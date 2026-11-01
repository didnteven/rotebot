//! Approved unattended triggers. Programs still stage once; only the host can
//! commit a local plan. Connection calls always remain staged for review.
use super::*;
use chrono::{DateTime, Datelike, Days, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use crate::conditions::SendWhen;
use serde::{Deserialize, Serialize};

pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str) -> Result<(), String>;
}

pub type MailFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>;
/// Sends one scheduled result as an email through a connected mailbox. The recipient was
/// approved with the schedule, so a program can never choose where its result goes.
pub trait ResultMailer: Send + Sync {
    fn send<'a>(
        &'a self,
        connector_id: &'a str,
        to: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> MailFuture<'a>;
}

/// Where a scheduled result is emailed. Part of the approved outputs: changing the
/// mailbox or the address needs a fresh approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailDelivery {
    pub connector_id: String,
    pub to: String,
}
impl EmailDelivery {
    /// One plain address only: no list, display name, spaces or line breaks, so the
    /// approved text is exactly who receives the result.
    fn validate(&self) -> CoreResult<()> {
        let (local, domain) = self.to.split_once('@').unwrap_or(("", ""));
        let plain = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c));
        if self.to.len() > 254
            || local.is_empty()
            || !plain(local)
            || !domain.contains('.')
            || !plain(domain)
            || domain.starts_with(['.', '-'])
            || domain.ends_with(['.', '-'])
            || self.connector_id.trim().is_empty()
        {
            return Err(CoreError::Invalid(
                "Enter one email address, like you@example.com".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frequency {
    Daily { time: String },
    Weekly { weekday: u32, time: String }, // Monday = 1
    Monthly { day: u32, time: String },
}
impl Frequency {
    fn time(&self) -> CoreResult<NaiveTime> {
        let text = match self {
            Self::Daily { time } | Self::Weekly { time, .. } | Self::Monthly { time, .. } => time,
        };
        if text.len() != 5
            || !text.bytes().enumerate().all(|(i, c)| {
                if i == 2 {
                    c == b':'
                } else {
                    c.is_ascii_digit()
                }
            })
        {
            return Err(CoreError::Invalid("Time must be HH:MM".into()));
        }
        NaiveTime::parse_from_str(text, "%H:%M")
            .map_err(|_| CoreError::Invalid("Time must be HH:MM".into()))
    }
    fn validate(&self) -> CoreResult<()> {
        self.time()?;
        if matches!(self, Self::Weekly { weekday, .. } if !(1..=7).contains(weekday))
            || matches!(self, Self::Monthly { day, .. } if !(1..=31).contains(day))
        {
            return Err(CoreError::Invalid(
                "Choose a valid weekday or day 1–31".into(),
            ));
        }
        Ok(())
    }
    fn matches(&self, date: NaiveDate) -> bool {
        match self {
            Self::Daily { .. } => true,
            Self::Weekly { weekday, .. } => date.weekday().number_from_monday() == *weekday,
            Self::Monthly { day, .. } => {
                date.day() == *day
                    || (date.day() < *day
                        && date.succ_opt().is_some_and(|d| d.month() != date.month()))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleOutputs {
    pub today: bool,
    pub notification: bool,
    pub folder: Option<FileHandle>,
    pub filename: String,
    #[serde(default)]
    pub email: Option<EmailDelivery>,
    /// Which runs send the notification and email. Today still records every run.
    #[serde(default, skip_serializing_if = "SendWhen::is_always")]
    pub send_when: SendWhen,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRequest {
    pub frequency: Frequency,
    pub outputs: ScheduleOutputs,
    pub version_hash: String,
    pub approved: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    pub tool_id: String,
    pub frequency: Frequency,
    pub outputs: ScheduleOutputs,
    pub version_hash: String,
    pub binding: String,
    pub folder_identity: Option<String>,
    pub status: String,
    pub approved_at_ms: i64,
    pub last_due_ms: i64,
    pub next_run_ms: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayEntry {
    pub id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub trigger: String,
    pub schedule_id: Option<String>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub status: String,
    pub body: String,
    pub note: Option<String>,
    pub file_created: Option<String>,
    pub run_id: Option<String>,
    pub staged: Option<StagedRun>,
    pub notification: bool,
    pub add_to_today: bool,
    pub dismissed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayDay {
    pub day: String,
    pub entries: Vec<TodayEntry>,
}

// A fall-back occurrence resolves to the earlier instant. A gap advances to the
// first representable local minute. Both are stable across repeated evaluations.
pub(crate) fn local_boundary(tz: Tz, mut local: chrono::NaiveDateTime) -> Option<DateTime<Tz>> {
    for _ in 0..=1440 {
        match tz.from_local_datetime(&local) {
            LocalResult::Single(d) => return Some(d),
            LocalResult::Ambiguous(a, b) => return Some(a.min(b)),
            LocalResult::None => local += chrono::Duration::minutes(1),
        }
    }
    None
}
fn occurrence(tz: Tz, day: NaiveDate, time: NaiveTime) -> Option<i64> {
    local_boundary(tz, day.and_time(time)).map(|d| d.timestamp_millis())
}
fn instant(now: i64, tz: Tz) -> DateTime<Tz> {
    Utc.timestamp_millis_opt(now)
        .single()
        .unwrap_or_default()
        .with_timezone(&tz)
}
/// Most recent occurrence, including now. One watermark coalesces all missed runs.
pub fn most_recent_due(f: &Frequency, now: i64, tz: Tz) -> CoreResult<Option<i64>> {
    f.validate()?;
    let mut day = instant(now, tz).date_naive();
    for _ in 0..=370 {
        if f.matches(day) {
            if let Some(due) = occurrence(tz, day, f.time()?) {
                if due <= now {
                    return Ok(Some(due));
                }
            }
        }
        let Some(previous) = day.pred_opt() else {
            break;
        };
        day = previous;
    }
    Ok(None)
}
fn next_due(f: &Frequency, now: i64, tz: Tz) -> CoreResult<Option<i64>> {
    let mut day = instant(now, tz).date_naive();
    for _ in 0..=370 {
        if f.matches(day) {
            if let Some(due) = occurrence(tz, day, f.time()?) {
                if due > now {
                    return Ok(Some(due));
                }
            }
        }
        let Some(next) = day.succ_opt() else { break };
        day = next;
    }
    Ok(None)
}

pub(crate) struct ActiveRun<'a> {
    core: &'a AppCore,
    tool: String,
}
impl Drop for ActiveRun<'_> {
    fn drop(&mut self) {
        lock(&self.core.inner.running_tools).remove(&self.tool);
    }
}
// Only host admission errors may classify a run as skipped or needing action.
// Runner failures have a separate variant and must remain failed regardless of
// words in their diagnostic detail. StageResult::Failed also bypasses this.
impl CoreError {
    pub(crate) fn today_failure_status(&self) -> &'static str {
        match self {
            CoreError::Invalid(message) if message.contains("Skipped") => "skipped",
            CoreError::Invalid(message) if message.contains("Reconnect") => "needs_you",
            _ => "failed",
        }
    }
}

impl AppCore {
    pub(crate) fn claim_tool_run(&self, tool: &str) -> CoreResult<ActiveRun<'_>> {
        if !lock(&self.inner.running_tools).insert(tool.into()) {
            return Err(CoreError::Invalid(
                "A run is already active for this tool".into(),
            ));
        }
        Ok(ActiveRun {
            core: self,
            tool: tool.into(),
        })
    }
    pub fn set_notifier(&self, notifier: Arc<dyn Notifier>) {
        *lock(&self.inner.notifier) = Some(notifier);
    }
    /// Injectable zone alongside CoreConfig's injectable clock. No global TZ mutation.
    pub fn set_schedule_timezone(&self, timezone: Tz) {
        *lock(&self.inner.timezone) = Some(timezone);
    }
    pub(crate) fn schedule_timezone(&self) -> Tz {
        lock(&self.inner.timezone).unwrap_or_else(|| {
            iana_time_zone::get_timezone()
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(chrono_tz::UTC)
        })
    }
    pub(crate) fn local_date(&self, now: i64) -> String {
        instant(now, self.schedule_timezone())
            .format("%Y-%m-%d")
            .to_string()
    }
    fn folder_identity(path: &Path) -> CoreResult<String> {
        SelectedRoot::from_picker(path).map_err(|e| CoreError::Invalid(e.to_string()))?;
        let m = std::fs::metadata(path).map_err(|e| CoreError::Invalid(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(format!("{}:{}", m.dev(), m.ino()))
        }
        #[cfg(not(unix))]
        {
            let _ = m;
            Err(CoreError::Invalid(
                "Scheduling unavailable on this platform".into(),
            ))
        }
    }
    fn schedule_binding(&self, tool: &str) -> CoreResult<String> {
        let row = self.db().tool(tool)?.ok_or(CoreError::NotFound("tool"))?;
        let detail = self.tool_detail(&row)?;
        let grant = self
            .db()
            .active_grant(tool, &detail.version_hash)?
            .ok_or_else(|| CoreError::Invalid("Tool access needs approval".into()))?;
        Ok(hash_hex(&broker::canonical_json(&json!({"version":detail.version_hash,"program":detail.program,"detail":row.detail,"grant":grant,"state":row.state})).map_err(|e| CoreError::Internal(e.to_string()))?))
    }
    fn put_schedule(&self, s: &Schedule) -> CoreResult<()> {
        self.db().put_schedule(&s.id, &s.tool_id, &json!(s))?;
        Ok(())
    }
    fn raw_schedules(&self) -> CoreResult<Vec<Schedule>> {
        self.db()
            .schedule_rows()?
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(|e| CoreError::Storage(e.to_string())))
            .collect()
    }
    pub(crate) fn valid_schedule(&self, s: &Schedule) -> bool {
        self.schedule_binding(&s.tool_id)
            .is_ok_and(|b| b == s.binding)
            && s.outputs.folder.as_ref().is_none_or(|f| {
                Self::folder_identity(Path::new(&f.display_path)).ok() == s.folder_identity
            })
    }
    pub(crate) fn schedule_still_approved(
        &self,
        s: &Schedule,
        require_enabled: bool,
    ) -> CoreResult<bool> {
        Ok(self.raw_schedules()?.iter().any(|current| {
            current.id == s.id
                && (!require_enabled || current.status == "enabled")
                && current.status != "needs_reapproval"
                && current.binding == s.binding
                && current.outputs == s.outputs
                && current.frequency == s.frequency
                && current.approved_at_ms == s.approved_at_ms
        }) && self.valid_schedule(s))
    }
    pub fn list_schedules(&self) -> CoreResult<Vec<Schedule>> {
        let _admission = self.admit_local()?;
        let mut schedules = self.raw_schedules()?;
        for s in &mut schedules {
            // An approved schedule follows its tool: a new version the person built, approved and saved
            // keeps running on the same schedule. Only a changed result folder still asks again.
            let folder_ok = s.outputs.folder.as_ref().is_none_or(|f| {
                Self::folder_identity(Path::new(&f.display_path)).ok() == s.folder_identity
            });
            let current_version = self.db().tool(&s.tool_id).ok().flatten()
                .and_then(|row| self.tool_detail(&row).ok()).map(|d| d.version_hash);
            // Only a new version (saved with its own approval) carries the schedule; the same version
            // with a changed approval record still asks again.
            if folder_ok && s.status != "emergency_paused" && current_version.as_ref().is_some_and(|v| *v != s.version_hash) {
                if let Ok(binding) = self.schedule_binding(&s.tool_id) {
                    if binding != s.binding {
                        s.binding = binding;
                        s.version_hash = current_version.clone().unwrap_or_default();
                        if s.status == "needs_reapproval" {
                            s.status = "enabled".into();
                        }
                        self.put_schedule(s)?;
                    }
                }
            }
            if !self.valid_schedule(s) {
                s.status = "needs_reapproval".into();
                self.put_schedule(s)?;
            }
            s.next_run_ms = if s.status == "enabled" {
                next_due(
                    &s.frequency,
                    self.now().max(s.last_due_ms),
                    self.schedule_timezone(),
                )?
            } else {
                None
            };
        }
        Ok(schedules)
    }
    pub fn set_schedule(&self, tool: &str, mut request: ScheduleRequest) -> CoreResult<Schedule> {
        let _admission = self.admit_local()?;
        request.frequency.validate()?;
        let detail = self.get_tool(tool)?;
        if !request.approved || detail.version_hash != request.version_hash {
            return Err(CoreError::Invalid(
                "Review and approve this tool version running unattended".into(),
            ));
        }
        if !detail.no_input {
            return Err(CoreError::Invalid(
                "Schedules require a tool without a selected input file".into(),
            ));
        }
        if let Some(email) = &request.outputs.email {
            email.validate()?;
        }
        request.outputs.send_when.validate()?;
        if !request.outputs.today
            && !request.outputs.notification
            && request.outputs.folder.is_none()
            && request.outputs.email.is_none()
        {
            return Err(CoreError::Invalid("Choose at least one output".into()));
        }
        let folder_identity = if let Some(folder) = &mut request.outputs.folder {
            let (path, handle) = self.handle_path(&folder.handle_id, PickKind::Dir)?;
            *folder = handle;
            broker::validate_basename(
                &request
                    .outputs
                    .filename
                    .replace("{date}", &self.local_date(self.now())),
            )
            .map_err(|e| CoreError::Invalid(e.to_string()))?;
            Some(Self::folder_identity(&path)?)
        } else {
            None
        };
        let old = self
            .raw_schedules()?
            .into_iter()
            .find(|s| s.tool_id == tool);
        let s = Schedule {
            id: old.map_or_else(|| new_id("schedule"), |s| s.id),
            tool_id: tool.into(),
            frequency: request.frequency,
            outputs: request.outputs,
            version_hash: request.version_hash,
            binding: self.schedule_binding(tool)?,
            folder_identity,
            status: "enabled".into(),
            approved_at_ms: self.now(),
            last_due_ms: self.now(),
            next_run_ms: None,
        };
        self.put_schedule(&s)?;
        Ok(Schedule {
            next_run_ms: next_due(
                &s.frequency,
                self.now().max(s.last_due_ms),
                self.schedule_timezone(),
            )?,
            ..s
        })
    }
    pub fn schedule_status(&self, tool: &str, status: &str) -> CoreResult<()> {
        let _admission = self.admit_local()?;
        if status == "remove" {
            self.db().remove_schedule(tool)?;
            return Ok(());
        }
        let mut s = self
            .raw_schedules()?
            .into_iter()
            .find(|s| s.tool_id == tool)
            .ok_or(CoreError::NotFound("schedule"))?;
        if !["enabled", "paused"].contains(&status) {
            return Err(CoreError::Invalid("Invalid schedule status".into()));
        }
        if status == "enabled" && (s.status == "needs_reapproval" || !self.valid_schedule(&s)) {
            return Err(CoreError::Invalid("Schedule needs re-approval".into()));
        }
        s.status = status.into();
        s.last_due_ms = self.now();
        self.put_schedule(&s)
    }
    pub(crate) fn pause_all_schedules(&self) -> CoreResult<()> {
        let _admission = self.admit_local()?;
        for mut s in self.raw_schedules()? {
            s.status = "emergency_paused".into();
            self.put_schedule(&s)?;
        }
        Ok(())
    }
    fn today_entries(&self) -> CoreResult<Vec<TodayEntry>> {
        self.db()
            .today_rows()?
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(|e| CoreError::Storage(e.to_string())))
            .collect()
    }
    pub(crate) fn expire_today(&self, now: i64) -> CoreResult<()> {
        let _admission = self.admit_local()?;
        let cutoff = now.saturating_sub(30 * 24 * 60 * 60 * 1_000);
        for entry in self.today_entries()? {
            if entry.status != "needs_you" && entry.started_at_ms < cutoff {
                self.db().dismiss_today(&entry.id)?;
            }
        }
        Ok(())
    }
    fn put_today(&self, e: &TodayEntry) -> CoreResult<()> {
        self.db().put_today(&e.id, &json!(e))?;
        Ok(())
    }
    pub fn list_today(&self) -> CoreResult<Vec<TodayDay>> {
        let _admission = self.admit_local()?;
        let today = instant(self.now(), self.schedule_timezone()).date_naive();
        let cutoff = today.checked_sub_days(Days::new(7)).unwrap_or(today);
        let mut groups = BTreeMap::<String, Vec<TodayEntry>>::new();
        for mut e in self.today_entries()? {
            if e.dismissed || (!e.add_to_today && e.status == "done") {
                continue;
            }
            let date = instant(e.started_at_ms, self.schedule_timezone()).date_naive();
            if (date < cutoff || date > today) && e.status != "needs_you" {
                continue;
            }
            if e.staged.is_some()
                && !lock(&self.inner.staged).contains_key(e.run_id.as_deref().unwrap_or_default())
            {
                e.status = "failed".into();
                e.body = "Interrupted while waiting for approval. Run the tool again.".into();
                e.staged = None;
                self.put_today(&e)?;
            }
            groups.entry(date.to_string()).or_default().push(e);
        }
        Ok(groups
            .into_iter()
            .rev()
            .map(|(day, mut entries)| {
                entries.sort_by_key(|e| std::cmp::Reverse(e.started_at_ms));
                TodayDay { day, entries }
            })
            .collect())
    }
    pub fn dismiss_today_entry(&self, id: &str) -> CoreResult<()> {
        let _admission = self.admit_local()?;
        let mut e = self
            .today_entries()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(CoreError::NotFound("Today entry"))?;
        if e.status == "needs_you" {
            return Err(CoreError::Invalid(
                "Approve or discard this run first".into(),
            ));
        }
        e.dismissed = true;
        self.put_today(&e)
    }
    pub(crate) fn manual_today_failure(
        &self,
        tool_id: &str,
        message: &str,
        status: &'static str,
    ) -> CoreResult<()> {
        let _admission = self.admit_local()?;
        let tool = self.get_tool(tool_id)?;
        if tool.no_input
            || tool
                .program
                .as_ref()
                .is_some_and(|p| p.model_step.is_some())
        {
            self.put_today(&TodayEntry {
                id: new_id("today"),
                tool_id: tool_id.into(),
                tool_name: tool.summary_row.name,
                trigger: "manual".into(),
                schedule_id: None,
                started_at_ms: self.now(),
                finished_at_ms: Some(self.now()),
                status: status.into(),
                body: message.chars().take(20_000).collect(),
                note: None,
                file_created: None,
                run_id: None,
                staged: None,
                notification: false,
                add_to_today: true,
                dismissed: false,
            })?;
        }
        Ok(())
    }
    pub(crate) fn stage_today(
        &self,
        dto: &StagedRun,
        tool: &ToolDetail,
        bytes: &[u8],
        schedule: Option<&Schedule>,
        started_at_ms: i64,
    ) -> CoreResult<()> {
        let e = TodayEntry {
            id: new_id("today"),
            tool_id: dto.tool_id.clone(),
            tool_name: tool.summary_row.name.clone(),
            trigger: if schedule.is_some() {
                "schedule"
            } else {
                "manual"
            }
            .into(),
            schedule_id: schedule.map(|s| s.id.clone()),
            started_at_ms,
            finished_at_ms: None,
            status: if dto.planned_calls.is_empty() {
                "staged"
            } else {
                "needs_you"
            }
            .into(),
            body: String::from_utf8_lossy(bytes)
                .chars()
                .take(20_000)
                .collect(),
            note: schedule
                .filter(|s| started_at_ms - s.last_due_ms >= 60_000)
                .map(|s| {
                    format!(
                        "Missed at {} (Macroscope was closed); ran at {}",
                        s.frequency
                            .time()
                            .map(|t| t.format("%-H:%M").to_string())
                            .unwrap_or_default(),
                        instant(started_at_ms, self.schedule_timezone()).format("%-H:%M")
                    )
                }),
            file_created: None,
            run_id: Some(dto.run_id.clone()),
            staged: Some(dto.clone()),
            notification: schedule.is_some_and(|s| s.outputs.notification),
            add_to_today: schedule.is_none_or(|s| s.outputs.today),
            dismissed: false,
        };
        self.put_today(&e)
    }
    pub(crate) fn run_trigger(&self, run: &str) -> CoreResult<(RunTrigger, Option<String>)> {
        Ok(self
            .today_entries()?
            .into_iter()
            .find(|e| e.run_id.as_deref() == Some(run))
            .map_or((RunTrigger::Manual, None), |e| {
                (
                    if e.trigger == "schedule" {
                        RunTrigger::Schedule
                    } else {
                        RunTrigger::Manual
                    },
                    e.schedule_id,
                )
            }))
    }
    fn notify_today(&self, e: &TodayEntry) {
        if !e.notification {
            return;
        }
        let body = match e.status.as_str() {
            "done" => e
                .body
                .lines()
                .next()
                .unwrap_or("Done")
                .chars()
                .take(120)
                .collect(),
            "needs_you" => format!(
                "Needs you: {} action{} to approve",
                e.staged.as_ref().map_or(0, |s| s.planned_calls.len()),
                if e.staged
                    .as_ref()
                    .is_some_and(|s| s.planned_calls.len() == 1)
                {
                    ""
                } else {
                    "s"
                }
            ),
            "skipped" => {
                if e.body.contains("Local only") {
                    "Skipped: Local only is on".into()
                } else {
                    e.body.chars().take(120).collect()
                }
            }
            // A program error may quote fetched data. Keep diagnostic text in
            // encrypted Today, rather than leaking it through the OS banner.
            _ => "Failed: Open Today for details".into(),
        };
        if let Some(n) = lock(&self.inner.notifier).as_ref() {
            if n.notify(&e.tool_name, &body).is_err() {
                self.activity("notification", "Notification could not be delivered", None);
            }
        }
    }
    pub(crate) fn finish_today(
        &self,
        run: &str,
        receipt: Option<&Receipt>,
        failure: Option<&str>,
    ) -> CoreResult<()> {
        if let Some(mut e) = self
            .today_entries()?
            .into_iter()
            .find(|e| e.run_id.as_deref() == Some(run))
        {
            e.finished_at_ms = Some(self.now());
            e.staged = None;
            if let Some(r) = receipt {
                e.status = if r.status == ReceiptStatus::Committed {
                    "done"
                } else {
                    "failed"
                }
                .into();
                e.file_created = (!r.output_path.is_empty()).then(|| r.output_path.clone());
                if e.status == "failed" {
                    e.body = format!("Connection actions finished with status {:?}", r.status);
                }
            }
            if let Some(f) = failure {
                e.status = if f == "Discarded" {
                    "skipped"
                } else {
                    "failed"
                }
                .into();
                e.body = f.chars().take(20_000).collect();
            }
            if e.status == "done" {
                if let Some(decision) = self.schedule_decision(&e) {
                    if !decision.send {
                        e.notification = false;
                    }
                    if let Some(note) = decision.note {
                        e.note = Some(match e.note.take() {
                            Some(existing) => format!("{existing}. {note}"),
                            None => note,
                        });
                    }
                }
            }
            self.put_today(&e)?;
            self.notify_today(&e);
        }
        Ok(())
    }
    /// Evaluated on launch and every 30s, including after wake/clock jumps.
    pub async fn run_due_schedules(&self, now: i64) -> CoreResult<Vec<TodayEntry>> {
        let _evaluation = self.inner.schedule_eval.lock().await;
        self.ensure_active()?;
        self.expire_today(now)?;
        // Publication can hold local admission through a slow external send.
        // Remember which tools were busy at evaluation, even if that send
        // finishes while this evaluator waits for admission.
        let busy_at_evaluation = lock(&self.inner.running_tools).clone();
        let mut ran = Vec::new();
        for mut s in self.list_schedules()? {
            if s.status != "enabled" {
                continue;
            }
            let tz = self.schedule_timezone();
            let Some(due) = most_recent_due(&s.frequency, now, tz)? else {
                continue;
            };
            if due <= s.last_due_ms {
                continue;
            }
            // Persist before fetching: re-entry and a crash cannot replay an occurrence.
            {
                let _admission = self.admit_local()?;
                if !self.schedule_still_approved(&s, true)? {
                    continue;
                }
                s.last_due_ms = due;
                self.put_schedule(&s)?;
            }
            let tool = self.get_tool(&s.tool_id)?;
            let time = s.frequency.time()?.format("%-H:%M").to_string();
            let mut entry = TodayEntry {
                id: new_id("today"),
                tool_id: s.tool_id.clone(),
                tool_name: tool.summary_row.name.clone(),
                trigger: "schedule".into(),
                schedule_id: Some(s.id.clone()),
                started_at_ms: now,
                finished_at_ms: Some(now),
                status: "failed".into(),
                body: String::new(),
                note: None,
                file_created: None,
                run_id: None,
                staged: None,
                notification: s.outputs.notification,
                add_to_today: s.outputs.today,
                dismissed: false,
            };
            if now - due >= 60_000 {
                entry.note = Some(format!(
                    "Missed at {time} (Macroscope was closed); ran at {}",
                    instant(now, tz).format("%-H:%M")
                ));
            }
            let active = busy_at_evaluation.contains(&s.tool_id)
                || lock(&self.inner.running_tools).contains(&s.tool_id)
                || lock(&self.inner.staged)
                    .values()
                    .any(|r| r.dto.tool_id == s.tool_id);
            let network = tool.program.as_ref().is_some_and(|p| {
                !p.web_sources.is_empty()
                    || !p.connection_sources.is_empty()
                    || p.model_step.is_some()
            });
            if active {
                entry.status = "skipped".into();
                entry.body = format!("Skipped at {time} — a run is already active");
            } else if tool
                .program
                .as_ref()
                .is_some_and(|p| p.model_step.is_some())
                && self.model_cap_reached(&tool)?
            {
                entry.status = "skipped".into();
                entry.body = "Skipped — monthly model-step cap reached".into();
            } else if network && self.db().setting("local_only")? == Some(json!(true)) {
                entry.status = "skipped".into();
                entry.body = format!("Skipped at {time} — Local only is on");
            } else {
                match self.stage_scheduled(&s, &tool, now).await {
                    Ok(StageResult::Staged(staged)) => {
                        entry = self
                            .today_entries()?
                            .into_iter()
                            .find(|e| e.run_id.as_deref() == Some(&staged.run_id))
                            .ok_or(CoreError::NotFound("Today entry"))?;
                        if !staged.planned_calls.is_empty() {
                            self.notify_today(&entry);
                            ran.push(entry);
                            continue;
                        }
                        // Recheck the stored approval after asynchronous source fetching.
                        let enabled = self.raw_schedules()?.iter().any(|current| {
                            current.id == s.id
                                && current.status == "enabled"
                                && current.binding == s.binding
                                && current.outputs == s.outputs
                                && current.frequency == s.frequency
                                && current.approved_at_ms == s.approved_at_ms
                        });
                        if !enabled || !self.valid_schedule(&s) {
                            self.discard_run(&staged.run_id)?;
                            continue;
                        }
                        self.commit_run(&staged.run_id, &staged.plan_hash)?;
                        entry = self
                            .today_entries()?
                            .into_iter()
                            .find(|e| e.run_id.as_deref() == Some(&staged.run_id))
                            .ok_or(CoreError::NotFound("Today entry"))?;
                        if entry.status == "done" {
                            self.deliver_email(&s, &mut entry).await;
                        }
                        ran.push(entry);
                        continue;
                    }
                    Ok(StageResult::Failed(f)) => entry.body = f.failure.message(),
                    Err(e) => {
                        entry.status = e.today_failure_status().into();
                        entry.body = e.to_string();
                    }
                }
            }
            let _admission = self.admit_local()?;
            self.put_today(&entry)?;
            self.notify_today(&entry);
            ran.push(entry);
        }
        Ok(ran)
    }
    /// What this schedule's "send only when" says about a finished run. `None` for manual
    /// runs and schedules that send every time.
    fn schedule_decision(&self, e: &TodayEntry) -> Option<crate::conditions::Decision> {
        if e.trigger != "schedule" {
            return None;
        }
        let id = e.schedule_id.as_deref()?;
        let schedule = self.raw_schedules().ok()?.into_iter().find(|s| s.id == id)?;
        let condition = &schedule.outputs.send_when;
        if condition.is_always() {
            return None;
        }
        // The last finished run of this schedule that is still kept, other than this one.
        let previous = self
            .today_entries()
            .ok()?
            .into_iter()
            .filter(|p| {
                p.schedule_id.as_deref() == Some(id)
                    && p.status == "done"
                    && p.run_id != e.run_id
                    && p.started_at_ms <= e.started_at_ms
            })
            .max_by_key(|p| p.started_at_ms)
            .map(|p| p.body);
        Some(condition.decide(&e.body, previous.as_deref()))
    }
    pub fn set_mailer(&self, mailer: Arc<dyn ResultMailer>) {
        *lock(&self.inner.mailer) = Some(mailer);
    }
    /// Emails a finished run to the address approved with the schedule. One attempt only:
    /// a send whose answer was lost is never repeated, and the outcome is written on the
    /// Today entry either way.
    async fn deliver_email(&self, s: &Schedule, entry: &mut TodayEntry) {
        let Some(email) = &s.outputs.email else {
            return;
        };
        if self.schedule_decision(entry).is_some_and(|d| !d.send) {
            return;
        }
        let outcome = self.send_result_email(s, email, entry).await;
        let note = match outcome {
            Ok(()) => format!("Emailed to {}", email.to),
            Err(reason) => format!("Email not sent: {reason}"),
        };
        entry.note = Some(match entry.note.take() {
            Some(existing) => format!("{existing}. {note}"),
            None => note,
        });
        if let Ok(_admission) = self.admit_local() {
            let _ = self.put_today(entry);
        }
    }
    async fn send_result_email(
        &self,
        s: &Schedule,
        email: &EmailDelivery,
        entry: &TodayEntry,
    ) -> Result<(), String> {
        if self.db().setting("local_only").ok().flatten() == Some(json!(true)) {
            return Err("Local only is on".into());
        }
        let mailer = lock(&self.inner.mailer)
            .clone()
            .ok_or("email is not available")?;
        // The person can pause or change the schedule while the tool runs.
        if !self.schedule_still_approved(s, true).map_err(|e| e.to_string())? {
            return Err("the schedule changed before the email was sent".into());
        }
        let text = entry.body.trim();
        if text.is_empty() {
            return Err("the tool produced no text to send".into());
        }
        let subject: String = format!(
            "{} · {}",
            entry.tool_name,
            self.local_date(entry.started_at_ms)
        )
        .chars()
        .filter(|c| !c.is_control())
        .take(150)
        .collect();
        let body: String = text.chars().take(20_000).collect::<String>()
            + "\n\n— Sent by your Macroscope schedule";
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            mailer.send(&email.connector_id, &email.to, &subject, &body),
        )
        .await
        .map_err(|_| "the mail server did not answer".to_string())?
    }
    async fn stage_scheduled(
        &self,
        s: &Schedule,
        tool: &ToolDetail,
        now: i64,
    ) -> CoreResult<StageResult> {
        let folder = if let Some(f) = &s.outputs.folder {
            self.register_picked_dir(Path::new(&f.display_path))?
        } else {
            let private = self.inner.cfg.data_dir.join("digest");
            std::fs::create_dir_all(&private).map_err(|e| CoreError::Internal(e.to_string()))?;
            self.register_picked_dir(&private)?
        };
        let original = if s.outputs.folder.is_some() {
            s.outputs.filename.replace("{date}", &self.local_date(now))
        } else {
            tool.program
                .as_ref()
                .ok_or(CoreError::NotFound("program"))?
                .output_name
                .clone()
        };
        let name = if s.outputs.folder.is_some() {
            let path = Path::new(&original);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            (1..=9)
                .map(|n| {
                    if n == 1 {
                        original.clone()
                    } else {
                        format!("{stem} ({n}).{ext}")
                    }
                })
                .find(|n| {
                    Path::new(&folder.display_path)
                        .join(n)
                        .symlink_metadata()
                        .is_err()
                })
                .ok_or_else(|| {
                    CoreError::Invalid(
                        "All file names through (9) already exist; no file was overwritten".into(),
                    )
                })?
        } else {
            original
        };
        self.stage_inner(&s.tool_id, None, &folder.handle_id, &name, Some(s))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn today_retention_deletes_rows_on_launch_and_schedule_evaluation() {
        use macroscope_vault::MemoryStore;
        use std::sync::atomic::{AtomicI64, Ordering};
        const DAY: i64 = 86_400_000;
        for on_launch in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let now = 1_800_000_000_000;
            let clock = Arc::new(AtomicI64::new(now - 50 * DAY));
            let vault = Arc::new(MemoryStore::default());
            let cfg = || CoreConfig {
                data_dir: tmp.path().join("data"),
                vault: vault.clone(),
                gateway_base_url: "http://127.0.0.1:9".into(),
                home: tmp.path().join("home"),
                muse: Arc::new(NativeMuse {
                    home: tmp.path().join("home"),
                }),
                app_version: "test".into(),
                platform: "macos".into(),
                now_ms: {
                    let c = clock.clone();
                    Arc::new(move || c.load(Ordering::SeqCst))
                },
            };
            let mut core = AppCore::new(cfg()).unwrap();
            for (id, age, status) in [
                ("kept", 29, "done"),
                ("boundary", 30, "failed"),
                ("expired", 31, "done"),
                ("dismissed", 31, "failed"),
                ("pending", 40, "needs_you"),
            ] {
                core.put_today(&TodayEntry {
                    id: id.into(),
                    tool_id: "test-tool".into(),
                    tool_name: "Test".into(),
                    trigger: "manual".into(),
                    schedule_id: None,
                    started_at_ms: now - age * DAY,
                    finished_at_ms: Some(now - age * DAY),
                    status: status.into(),
                    body: "retained payload".into(),
                    note: None,
                    file_created: None,
                    run_id: None,
                    staged: None,
                    notification: false,
                    add_to_today: true,
                    dismissed: id == "dismissed",
                })
                .unwrap();
            }
            clock.store(now, Ordering::SeqCst);
            if on_launch {
                drop(core);
                core = AppCore::new(cfg()).unwrap();
            } else {
                assert!(core
                    .run_due_schedules(clock.load(Ordering::SeqCst))
                    .await
                    .unwrap()
                    .is_empty());
            }
            let mut ids: Vec<_> = core
                .today_entries()
                .unwrap()
                .into_iter()
                .map(|e| e.id)
                .collect();
            ids.sort();
            assert_eq!(ids, ["boundary", "kept", "pending"], "launch={on_launch}");
            assert_eq!(core.list_today().unwrap()[0].entries[0].id, "pending");
            let overview = core.privacy_overview().await.unwrap();
            let today = overview
                .stored
                .iter()
                .find(|s| s.label == "Today entries")
                .unwrap();
            assert_eq!(today.count, 3);
            assert_eq!(
                today.expires,
                "30 days; items needing you stay until handled"
            );
            // Once handled, the age exemption ends at the next evaluation.
            let mut pending = core
                .today_entries()
                .unwrap()
                .into_iter()
                .find(|e| e.id == "pending")
                .unwrap();
            pending.status = "done".into();
            core.put_today(&pending).unwrap();
            core.run_due_schedules(now).await.unwrap();
            assert_eq!(core.today_entries().unwrap().len(), 2);
        }
    }

    fn utc(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    #[test]
    fn daily_weekly_and_month_end() {
        let tz = chrono_tz::Pacific::Auckland;
        let daily = Frequency::Daily {
            time: "07:00".into(),
        };
        assert_eq!(
            most_recent_due(&daily, utc("2026-10-28T06:59:00+13:00"), tz).unwrap(),
            Some(utc("2026-10-27T07:00:00+13:00"))
        );
        assert_eq!(
            most_recent_due(&daily, utc("2026-10-28T07:00:00+13:00"), tz).unwrap(),
            Some(utc("2026-10-28T07:00:00+13:00"))
        );
        let weekly = Frequency::Weekly {
            weekday: 3,
            time: "07:00".into(),
        };
        assert_eq!(
            most_recent_due(&weekly, utc("2026-10-29T09:00:00+13:00"), tz).unwrap(),
            Some(utc("2026-10-28T07:00:00+13:00"))
        );
        let monthly = Frequency::Monthly {
            day: 31,
            time: "07:00".into(),
        };
        for (now, expected) in [
            ("2026-11-30T07:00:00+13:00", "2026-11-30T07:00:00+13:00"),
            ("2026-02-28T07:00:00+13:00", "2026-02-28T07:00:00+13:00"),
            ("2028-02-29T07:00:00+13:00", "2028-02-29T07:00:00+13:00"),
        ] {
            assert_eq!(
                most_recent_due(&monthly, utc(now), tz).unwrap(),
                Some(utc(expected))
            );
        }
    }
    #[test]
    fn spring_gap_runs_first_valid_minute_and_fall_repeat_once() {
        let tz = chrono_tz::Pacific::Auckland;
        let f = Frequency::Daily {
            time: "02:30".into(),
        };
        assert_eq!(
            most_recent_due(&f, utc("2026-09-27T03:00:00+13:00"), tz).unwrap(),
            Some(utc("2026-09-27T03:00:00+13:00"))
        );
        let first = utc("2026-04-05T02:30:00+13:00");
        assert_eq!(most_recent_due(&f, first, tz).unwrap(), Some(first));
        assert_eq!(
            most_recent_due(&f, utc("2026-04-05T02:30:00+12:00"), tz).unwrap(),
            Some(first)
        );
        assert_eq!(
            next_due(&f, first, tz).unwrap(),
            Some(utc("2026-04-06T02:30:00+12:00"))
        );
    }
    #[test]
    fn reject_invalid_schedule_fields() {
        for f in [
            Frequency::Daily {
                time: "7:00".into(),
            },
            Frequency::Daily {
                time: "24:00".into(),
            },
            Frequency::Weekly {
                weekday: 0,
                time: "07:00".into(),
            },
            Frequency::Monthly {
                day: 32,
                time: "07:00".into(),
            },
        ] {
            assert!(f.validate().is_err());
        }
    }
}
