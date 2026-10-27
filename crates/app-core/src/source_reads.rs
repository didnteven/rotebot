//! Approved host reads with local-time templates. No credentials cross the runner boundary.
use crate::*;
use chrono::Datelike;

impl AppCore {
    /// One local calendar month for caps, model spend and build spend alike.
    pub(crate) fn month_window(&self) -> CoreResult<(String, i64)> {
        let tz = self.schedule_timezone();
        let now = chrono::DateTime::from_timestamp_millis(self.now())
            .ok_or_else(|| CoreError::Invalid("Invalid clock".into()))?
            .with_timezone(&tz);
        let day = now.date_naive().with_day(1).unwrap();
        let start = schedules::local_boundary(tz, day.and_hms_opt(0, 0, 0).unwrap())
            .ok_or_else(|| CoreError::Invalid("Invalid local month".into()))?;
        Ok((day.format("%Y-%m").to_string(), start.timestamp_millis()))
    }
    pub(crate) fn model_counter_key(&self, tool: &str) -> CoreResult<String> {
        Ok(format!("model-step:{tool}:{}", self.month_window()?.0))
    }
    pub fn set_model_step_cap(&self, tool: &str, cap: u32) -> CoreResult<ToolDetail> {
        let _admission = self.admit_local()?;
        if cap > 10_000 {
            return Err(CoreError::Invalid("Monthly cap must be 0–10000".into()));
        }
        let mut row = self.db().tool(tool)?.ok_or(CoreError::NotFound("tool"))?;
        row.detail["modelStepCap"] = json!(cap);
        self.db().put_tool(&row)?;
        self.tool_detail(&row)
    }
    pub(crate) fn model_cap_reached(&self, tool: &ToolDetail) -> CoreResult<bool> {
        let n = self
            .db()
            .setting(&self.model_counter_key(&tool.summary_row.id)?)?
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cap = self
            .db()
            .tool(&tool.summary_row.id)?
            .and_then(|t| t.detail.get("modelStepCap").and_then(Value::as_u64))
            .unwrap_or(60);
        Ok(n >= cap)
    }

    pub fn source_args(&self, template: &str) -> CoreResult<Value> {
        let tz = self.schedule_timezone();
        let now = chrono::DateTime::from_timestamp_millis(self.now())
            .ok_or_else(|| CoreError::Invalid("Invalid clock".into()))?
            .with_timezone(&tz);
        let day = now.date_naive();
        let month = day.with_day(1).unwrap();
        let next = if month.month() == 12 {
            chrono::NaiveDate::from_ymd_opt(month.year() + 1, 1, 1).unwrap()
        } else {
            month.with_month(month.month() + 1).unwrap()
        };
        let prev = month.pred_opt().unwrap().with_day(1).unwrap();
        let boundary = |d: chrono::NaiveDate, end: bool| -> CoreResult<String> {
            let time = d
                .and_hms_opt(
                    if end { 23 } else { 0 },
                    if end { 59 } else { 0 },
                    if end { 59 } else { 0 },
                )
                .unwrap();
            schedules::local_boundary(tz, time)
                .map(|d| d.to_rfc3339())
                .ok_or_else(|| CoreError::Invalid("Source boundary falls in a timezone gap".into()))
        };
        let replacements = [
            ("{today}", day.to_string()),
            ("{now}", now.to_rfc3339()),
            ("{today_start}", boundary(day, false)?),
            ("{today_end}", boundary(day, true)?),
            ("{month_start}", boundary(month, false)?),
            ("{month_end}", boundary(next.pred_opt().unwrap(), true)?),
            ("{prev_month_start}", boundary(prev, false)?),
            (
                "{prev_month_end}",
                boundary(month.pred_opt().unwrap(), true)?,
            ),
        ];
        fn substitute(v: &mut Value, r: &[(&str, String)]) {
            match v {
                Value::String(s) => {
                    for (key, value) in r {
                        *s = s.replace(key, value);
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        substitute(v, r);
                    }
                }
                Value::Object(m) => {
                    for v in m.values_mut() {
                        substitute(v, r);
                    }
                }
                _ => {}
            }
        }
        let mut args: Value = serde_json::from_str(template)
            .map_err(|_| CoreError::Invalid("Source args must be JSON".into()))?;
        substitute(&mut args, &replacements);
        Ok(args)
    }

    pub(crate) async fn fetch_connection_sources(
        &self,
        p: &macroscope_builder::Program,
        selected: &[SelectedConnection],
    ) -> CoreResult<BTreeMap<String, String>> {
        if p.connection_sources.is_empty() {
            return Ok(BTreeMap::new());
        }
        if self.web_fetch_blocked().is_some() {
            return Err(CoreError::Invalid(
                "Local only blocks connection reads".into(),
            ));
        }
        connections::validate_sources(p, selected).map_err(CoreError::Invalid)?;
        let epoch = self.inner.policy.epoch();
        let connections =
            connections::Connections::new(self.clone(), self.inner.cfg.vault.clone())?;
        tokio::select! {
            biased;
            _ = self.inner.policy.revoked(epoch) => Err(CoreError::Invalid("Connection reads cancelled".into())),
            result = connections.read_sources(&p.connection_sources, selected) => result,
        }
    }
}
