//! Consent epochs close admission before cancellation. Acknowledgment is a host
//! obligation: it means the worker's transports/children have actually stopped.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EgressState {
    CloudAllowed,
    Stopping,
    LocalOnly,
    KeyUnavailable,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("egress is revoked or its consent epoch is stale")]
    Revoked,
    #[error("revocation barrier is still pending")]
    BarrierPending,
    #[error("consent epoch exhausted")]
    EpochExhausted,
    #[error("barrier acknowledgment does not match pending revocation")]
    InvalidAcknowledgment,
}
pub type Denied = PolicyError;
#[derive(Debug)]
struct Inner {
    state: EgressState,
    epoch: u64,
    next: u64,
    workers: HashSet<u64>,
    failed: bool,
}
#[derive(Debug, Clone)]
pub struct EgressPolicy {
    inner: Arc<Mutex<Inner>>,
    notify: Arc<Notify>,
}
impl Default for EgressPolicy {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: EgressState::LocalOnly,
                epoch: 0,
                next: 0,
                workers: HashSet::new(),
                failed: false,
            })),
            notify: Arc::new(Notify::new()),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Job {
    pub epoch: u64,
}
#[derive(Debug)]
pub struct Admission {
    pub epoch: u64,
    pub guard: EpochGuard,
}
#[derive(Debug)]
pub struct EpochGuard {
    policy: EgressPolicy,
    epoch: u64,
    worker: u64,
}
#[derive(Debug)]
pub struct LocalOnlyBarrier {
    policy: EgressPolicy,
    epoch: u64,
}
impl EgressPolicy {
    pub fn state(&self) -> EgressState {
        self.inner.lock().unwrap().state
    }
    pub fn epoch(&self) -> u64 {
        self.inner.lock().unwrap().epoch
    }
    pub fn allow_cloud(&self) -> Result<u64, PolicyError> {
        let mut i = self.inner.lock().unwrap();
        if i.state == EgressState::Stopping {
            return Err(PolicyError::BarrierPending);
        }
        i.epoch = i.epoch.checked_add(1).ok_or(PolicyError::EpochExhausted)?;
        i.state = EgressState::CloudAllowed;
        Ok(i.epoch)
    }
    pub fn authorize_dispatch(&self, epoch: u64) -> Result<(), Denied> {
        let i = self.inner.lock().unwrap();
        if i.state == EgressState::CloudAllowed && epoch == i.epoch {
            Ok(())
        } else {
            Err(PolicyError::Revoked)
        }
    }
    /// Completes when this job's dispatch authority has been revoked.
    pub async fn revoked(&self, epoch: u64) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.authorize_dispatch(epoch).is_err() {
                return;
            }
            notified.await;
        }
    }
    pub fn admit(&self, job: Job) -> Result<Admission, Denied> {
        let mut i = self.inner.lock().unwrap();
        if i.state != EgressState::CloudAllowed || job.epoch != i.epoch {
            return Err(PolicyError::Revoked);
        }
        i.next = i.next.checked_add(1).ok_or(PolicyError::EpochExhausted)?;
        let worker = i.next;
        i.workers.insert(worker);
        Ok(Admission {
            epoch: i.epoch,
            guard: EpochGuard {
                policy: self.clone(),
                epoch: i.epoch,
                worker,
            },
        })
    }
    pub fn begin_local_only(&self) -> Result<LocalOnlyBarrier, PolicyError> {
        let mut i = self.inner.lock().unwrap();
        if i.state != EgressState::Stopping {
            i.epoch = i.epoch.checked_add(1).ok_or(PolicyError::EpochExhausted)?;
            i.state = EgressState::Stopping;
        }
        let epoch = i.epoch;
        if i.workers.is_empty() && !i.failed {
            i.state = EgressState::LocalOnly;
        }
        self.notify.notify_waiters();
        Ok(LocalOnlyBarrier {
            policy: self.clone(),
            epoch,
        })
    }
    pub fn mark_key_unavailable(&self) -> Result<(), PolicyError> {
        let mut i = self.inner.lock().unwrap();
        if matches!(i.state, EgressState::CloudAllowed | EgressState::Stopping) {
            return Err(PolicyError::BarrierPending);
        }
        i.state = EgressState::KeyUnavailable;
        Ok(())
    }
}
impl EpochGuard {
    pub fn check(&self) -> Result<(), Denied> {
        self.policy.authorize_dispatch(self.epoch)
    }
    /// Call only after this worker can no longer dispatch. Dropping a guard does
    /// not acknowledge revocation; a lost worker must keep the barrier blocked.
    pub fn acknowledge(self, success: bool) {
        let mut i = self.policy.inner.lock().unwrap();
        if !success {
            i.failed = true;
        }
        if success {
            i.workers.remove(&self.worker);
        }
        if i.state == EgressState::Stopping && i.workers.is_empty() && !i.failed {
            i.state = EgressState::LocalOnly;
        }
        self.policy.notify.notify_waiters();
    }
}
impl LocalOnlyBarrier {
    pub fn is_resolved(&self) -> bool {
        let i = self.policy.inner.lock().unwrap();
        i.epoch == self.epoch && i.state == EgressState::LocalOnly
    }
    pub async fn wait(&self) -> Result<(), PolicyError> {
        loop {
            let notified = self.policy.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let i = self.policy.inner.lock().unwrap();
                if i.epoch != self.epoch {
                    return Err(PolicyError::Revoked);
                }
                if i.failed {
                    return Err(PolicyError::BarrierPending);
                }
                if i.state == EgressState::LocalOnly {
                    return Ok(());
                }
            }
            notified.await;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn admission_closes_stale_dispatch_refused_barrier_waits_for_all_acks() {
        let p = EgressPolicy::default();
        assert!(p.admit(Job { epoch: 0 }).is_err());
        let e = p.allow_cloud().unwrap();
        let a = p.admit(Job { epoch: e }).unwrap();
        let b = p.admit(Job { epoch: e }).unwrap();
        a.guard.check().unwrap();
        let barrier = p.begin_local_only().unwrap();
        assert_eq!(p.epoch(), e + 1);
        assert!(p.admit(Job { epoch: e + 1 }).is_err());
        assert!(a.guard.check().is_err());
        assert!(!barrier.is_resolved());
        assert_eq!(p.allow_cloud(), Err(PolicyError::BarrierPending));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), barrier.wait())
                .await
                .is_err()
        );
        a.guard.acknowledge(true);
        assert!(!barrier.is_resolved());
        b.guard.acknowledge(true);
        barrier.wait().await.unwrap();
        assert_eq!(p.state(), EgressState::LocalOnly);
        p.mark_key_unavailable().unwrap();
        p.allow_cloud().unwrap();
        assert!(p.authorize_dispatch(e).is_err());
    }
    #[tokio::test]
    async fn failed_or_lost_ack_keeps_stopping() {
        let p = EgressPolicy::default();
        let e = p.allow_cloud().unwrap();
        let a = p.admit(Job { epoch: e }).unwrap();
        let barrier = p.begin_local_only().unwrap();
        a.guard.acknowledge(false);
        assert_eq!(barrier.wait().await, Err(PolicyError::BarrierPending));
        assert_eq!(p.state(), EgressState::Stopping);
        assert!(!p.begin_local_only().unwrap().is_resolved());
    }
    #[test]
    fn epoch_never_wraps() {
        let p = EgressPolicy::default();
        p.inner.lock().unwrap().epoch = u64::MAX;
        assert_eq!(p.allow_cloud(), Err(PolicyError::EpochExhausted));
        assert_eq!(p.state(), EgressState::LocalOnly);
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;
    #[test]
    fn failure_before_revocation_and_dropped_guard_still_block_barrier() {
        let policy = EgressPolicy::default();
        let epoch = policy.allow_cloud().unwrap();
        let worker = policy.admit(Job { epoch }).unwrap();
        worker.guard.acknowledge(false);
        assert!(!policy.begin_local_only().unwrap().is_resolved());
        assert_eq!(policy.state(), EgressState::Stopping);
        let policy = EgressPolicy::default();
        let epoch = policy.allow_cloud().unwrap();
        let worker = policy.admit(Job { epoch }).unwrap();
        drop(worker);
        assert!(!policy.begin_local_only().unwrap().is_resolved());
        assert_eq!(policy.state(), EgressState::Stopping);
    }
}
