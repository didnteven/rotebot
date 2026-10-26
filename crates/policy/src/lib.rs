//! Egress consent epoch skeleton. A barrier acknowledgment is a host obligation,
//! never proof by itself that worker processes or transports have been stopped.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EgressState {
    CloudAllowed,
    Stopping,
    LocalOnly,
    KeyUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressPolicy {
    state: EgressState,
    epoch: u64,
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

impl Default for EgressPolicy {
    fn default() -> Self {
        Self {
            state: EgressState::LocalOnly,
            epoch: 0,
        }
    }
}

impl EgressPolicy {
    pub fn state(&self) -> EgressState {
        self.state
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Called only after explicit host-reviewed cloud consent and usable credentials.
    pub fn allow_cloud(&mut self) -> Result<u64, PolicyError> {
        if self.state == EgressState::Stopping {
            return Err(PolicyError::BarrierPending);
        }
        self.advance_epoch()?;
        self.state = EgressState::CloudAllowed;
        Ok(self.epoch)
    }

    pub fn authorize_dispatch(&self, job_epoch: u64) -> Result<(), PolicyError> {
        if self.state == EgressState::CloudAllowed && job_epoch == self.epoch {
            Ok(())
        } else {
            Err(PolicyError::Revoked)
        }
    }

    /// Close admission before the host cancels/reaps workers. Idempotent while stopping.
    pub fn begin_revocation(&mut self) -> Result<u64, PolicyError> {
        if self.state != EgressState::Stopping {
            self.advance_epoch()?;
            self.state = EgressState::Stopping;
        }
        Ok(self.epoch)
    }

    /// Host calls this only after every worker acknowledges and can no longer dispatch.
    pub fn acknowledge_local_only(&mut self, barrier_epoch: u64) -> Result<(), PolicyError> {
        if self.state != EgressState::Stopping || self.epoch != barrier_epoch {
            return Err(PolicyError::InvalidAcknowledgment);
        }
        self.state = EgressState::LocalOnly;
        Ok(())
    }

    /// Missing credentials can be recorded after revocation has completed.
    pub fn mark_key_unavailable(&mut self) -> Result<(), PolicyError> {
        if self.state == EgressState::Stopping || self.state == EgressState::CloudAllowed {
            return Err(PolicyError::BarrierPending);
        }
        self.state = EgressState::KeyUnavailable;
        Ok(())
    }

    fn advance_epoch(&mut self) -> Result<(), PolicyError> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or(PolicyError::EpochExhausted)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revocation_closes_admission_before_acknowledgment_and_stales_old_jobs() {
        let mut policy = EgressPolicy::default();
        assert_eq!(policy.authorize_dispatch(0), Err(PolicyError::Revoked));
        let old = policy.allow_cloud().unwrap();
        policy.authorize_dispatch(old).unwrap();
        let barrier = policy.begin_revocation().unwrap();
        assert_eq!(barrier, old + 1);
        assert_eq!(policy.begin_revocation().unwrap(), barrier);
        assert_eq!(policy.state(), EgressState::Stopping);
        assert_eq!(policy.authorize_dispatch(old), Err(PolicyError::Revoked));
        assert_eq!(
            policy.authorize_dispatch(barrier),
            Err(PolicyError::Revoked)
        );
        assert_eq!(policy.allow_cloud(), Err(PolicyError::BarrierPending));
        assert_eq!(
            policy.acknowledge_local_only(old),
            Err(PolicyError::InvalidAcknowledgment)
        );
        policy.acknowledge_local_only(barrier).unwrap();
        policy.mark_key_unavailable().unwrap();
        assert_eq!(policy.state(), EgressState::KeyUnavailable);
        policy.allow_cloud().unwrap();
        assert_eq!(policy.authorize_dispatch(old), Err(PolicyError::Revoked));
    }

    #[test]
    fn epoch_never_wraps() {
        let mut policy = EgressPolicy {
            state: EgressState::LocalOnly,
            epoch: u64::MAX,
        };
        assert_eq!(policy.allow_cloud(), Err(PolicyError::EpochExhausted));
        assert_eq!(policy.state(), EgressState::LocalOnly);
    }
}
