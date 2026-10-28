//! Custom endpoint configuration contains no credential bytes. Only the vault owns the token.
use crate::*;
use macroscope_model_gateway::{
    validate_custom_url, CustomCallObserver, CustomEndpointClient, StructuredRequest, Usage,
};

impl AppCore {
    pub(crate) fn custom_token_ref() -> SecretRef {
        SecretRef::new("custom-endpoint-token").expect("static vault item")
    }
    fn custom_endpoint_status_result(&self) -> CoreResult<CustomEndpointStatus> {
        let stored = self.db().setting("custom_endpoint")?;
        let mut c: CustomEndpointStatus = match stored.filter(|v| !v.is_null()) {
            Some(v) => serde_json::from_value(v)
                .map_err(|_| CoreError::Invalid("Custom endpoint settings are invalid".into()))?,
            None => CustomEndpointStatus {
                monthly_call_cap: 500,
                ..Default::default()
            },
        };
        c.token_present = matches!(
            self.inner.cfg.vault.get(&Self::custom_token_ref()),
            Ok(Some(_))
        );
        c.calls_this_month = self
            .db()
            .setting(&self.custom_counter_key()?)?
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        Ok(c)
    }
    pub(crate) fn custom_endpoint_status(&self) -> CustomEndpointStatus {
        self.custom_endpoint_status_result().unwrap_or_default()
    }
    pub(crate) fn custom_ready(&self) -> bool {
        let c = self.custom_endpoint_status();
        c.base_url.is_some()
            && c.model.is_some()
            && c.token_present
            && c.last_test.is_some_and(|t| t.status == "ok")
    }
    pub(crate) fn custom_route_label(&self) -> String {
        let c = self.custom_endpoint_status();
        let model = c.model.unwrap_or_default();
        format!(
            "Custom endpoint{} · {} · {}",
            c.display_name
                .map(|s| format!(" ({s})"))
                .unwrap_or_default(),
            model.rsplit('/').next().unwrap_or(&model),
            c.host.unwrap_or_default()
        )
    }
    fn store_custom(&self, c: &CustomEndpointStatus) -> CoreResult<()> {
        self.db().set_setting("custom_endpoint", &json!(c))?;
        Ok(())
    }
    pub async fn save_custom_endpoint(
        &self,
        base_url: &str,
        model: &str,
    ) -> CoreResult<ModelAccessStatus> {
        let url =
            validate_custom_url(base_url.trim(), self.allow_insecure_web()).map_err(|_| {
                CoreError::Invalid(
                    "Use an HTTPS base URL without userinfo, query or fragment.".into(),
                )
            })?;
        CustomEndpointClient::new(url.as_str(), model.trim(), self.allow_insecure_web())
            .map_err(|_| CoreError::Invalid("Enter a model name (up to 256 characters).".into()))?;
        let _control = self.inner.cloud_controls.lock().await;
        let reopen = self.inner.policy.state() == PolicyState::CloudAllowed;
        self.stop_cloud_work().await?;
        let mut c = self.custom_endpoint_status();
        if c.base_url.as_deref() != Some(url.as_str()) || c.model.as_deref() != Some(model.trim()) {
            c.last_test = None;
        }
        c.base_url = Some(url.to_string());
        c.host = url.host_str().map(String::from);
        c.model = Some(model.trim().into());
        self.store_custom(&c)?;
        if reopen {
            self.inner
                .policy
                .allow_cloud()
                .map_err(|e| CoreError::Internal(e.to_string()))?;
        }
        Ok(self.model_access().await)
    }
    pub async fn set_custom_endpoint_options(
        &self,
        display_name: Option<String>,
        monthly_call_cap: u32,
    ) -> CoreResult<ModelAccessStatus> {
        if display_name
            .as_ref()
            .is_some_and(|s| s.len() > 100 || s.chars().any(char::is_control))
        {
            return Err(CoreError::Invalid(
                "Use a display name up to 100 characters.".into(),
            ));
        }
        let _control = self.inner.cloud_controls.lock().await;
        {
            let _calls = lock(&self.inner.custom_calls);
            let mut c = self.custom_endpoint_status();
            c.display_name = display_name.filter(|s| !s.trim().is_empty());
            c.monthly_call_cap = monthly_call_cap;
            self.store_custom(&c)?;
        }
        Ok(self.model_access().await)
    }
    /// Called by the native dialog (and native test/eval code), never by token-bearing IPC.
    pub async fn save_custom_endpoint_token(&self, raw: &str) -> CoreResult<ModelAccessStatus> {
        let (secret, _) = validate_key(raw)
            .map_err(|_| CoreError::Invalid("That token is empty or has line breaks.".into()))?;
        let _control = self.inner.cloud_controls.lock().await;
        let reopen = self.inner.policy.state() == PolicyState::CloudAllowed;
        self.stop_cloud_work().await?;
        self.inner
            .cfg
            .vault
            .set(&Self::custom_token_ref(), secret)
            .map_err(|_| CoreError::Vault("Token could not be saved".into()))?;
        let mut c = self.custom_endpoint_status();
        c.last_test = None;
        self.store_custom(&c)?;
        if reopen {
            self.inner
                .policy
                .allow_cloud()
                .map_err(|e| CoreError::Internal(e.to_string()))?;
        }
        Ok(self.model_access().await)
    }
    pub async fn remove_custom_endpoint(&self) -> CoreResult<ModelAccessStatus> {
        let _control = self.inner.cloud_controls.lock().await;
        let reopen = self.inner.policy.state() == PolicyState::CloudAllowed;
        self.stop_cloud_work().await?;
        self.inner
            .cfg
            .vault
            .delete(&Self::custom_token_ref())
            .map_err(|_| CoreError::Vault("Token could not be removed".into()))?;
        self.db().set_setting("custom_endpoint", &Value::Null)?;
        if reopen {
            self.inner
                .policy
                .allow_cloud()
                .map_err(|e| CoreError::Internal(e.to_string()))?;
        }
        Ok(self.model_access().await)
    }
    pub async fn test_custom_endpoint(&self) -> CoreResult<ModelAccessStatus> {
        let c = self.custom_endpoint_status();
        let client = CustomEndpointClient::new(
            c.base_url.as_deref().unwrap_or(""),
            c.model.as_deref().unwrap_or(""),
            self.allow_insecure_web(),
        )
        .map_err(|_| CoreError::Invalid("Set URL and model first.".into()))?
        .with_observer(self.custom_observer("Endpoint test", true));
        let key = self
            .inner
            .cfg
            .vault
            .get(&Self::custom_token_ref())
            .map_err(|_| CoreError::Vault("Token unavailable".into()))?
            .ok_or_else(|| CoreError::Invalid("Set a token first.".into()))?;
        let admission = self
            .inner
            .policy
            .admit(macroscope_policy::Job {
                epoch: self.inner.policy.epoch(),
            })
            .map_err(|_| CoreError::Invalid("Local only blocks endpoint tests".into()))?;
        let schema = json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false});
        let start = std::time::Instant::now();
        let result = tokio::select! { biased;
            _ = self.inner.policy.revoked(admission.epoch) => Err(GatewayError::Unavailable),
            r = client.generate_structured(StructuredRequest { key:&key, model:None, system:"Return the requested JSON object.", user:"Return ok: true.", max_output_tokens:128, reasoning_effort:"medium", schema_name:"endpoint_test", schema:&schema }) => r,
        };
        if self
            .inner
            .policy
            .authorize_dispatch(admission.epoch)
            .is_err()
        {
            admission.guard.acknowledge(true);
            return Err(CoreError::Invalid("Endpoint test cancelled".into()));
        }
        if matches!(result, Err(GatewayError::CallCapReached)) {
            admission.guard.acknowledge(true);
            return Err(CoreError::Invalid(GatewayError::CallCapReached.to_string()));
        }
        let status = match result {
            Ok(_) => "ok",
            Err(GatewayError::AuthRejected) => "token rejected",
            Err(
                GatewayError::Rejected | GatewayError::MalformedOutput | GatewayError::Truncated,
            ) => "schema unsupported",
            _ => "unreachable",
        };
        let mut c = self.custom_endpoint_status();
        c.last_test = Some(CustomEndpointTest {
            status: status.into(),
            latency_ms: start.elapsed().as_millis() as u64,
        });
        self.store_custom(&c)?;
        admission.guard.acknowledge(true);
        Ok(self.model_access().await)
    }
    fn custom_counter_key(&self) -> CoreResult<String> {
        Ok(format!("custom_calls_{}", self.month_window()?.0))
    }
    pub(crate) fn check_custom_cap(&self) -> CoreResult<()> {
        let c = self.custom_endpoint_status_result()?;
        if c.calls_this_month >= u64::from(c.monthly_call_cap) {
            return Err(CoreError::Invalid(GatewayError::CallCapReached.to_string()));
        }
        Ok(())
    }
    pub(crate) fn custom_observer(
        &self,
        purpose: &'static str,
        synthetic_only: bool,
    ) -> Arc<dyn CustomCallObserver> {
        Arc::new(CallLog {
            core: self.clone(),
            purpose,
            synthetic_only,
        })
    }
}
struct CallLog {
    core: AppCore,
    purpose: &'static str,
    synthetic_only: bool,
}
impl CustomCallObserver for CallLog {
    fn dispatch(&self, request_bytes: usize) -> Result<String, GatewayError> {
        let _calls = lock(&self.core.inner.custom_calls);
        self.core
            .check_custom_cap()
            .map_err(|_| GatewayError::CallCapReached)?;
        let c = self
            .core
            .custom_endpoint_status_result()
            .map_err(|_| GatewayError::Unavailable)?;
        let key = self
            .core
            .custom_counter_key()
            .map_err(|_| GatewayError::Unavailable)?;
        self.core
            .db()
            .set_setting(&key, &json!(c.calls_this_month + 1))
            .map_err(|_| GatewayError::Unavailable)?;
        let id = new_id("custom_call");
        self.core
            .db()
            .insert_outbound(&db::OutboundRow {
                id: id.clone(),
                at_ms: self.core.now(),
                purpose: self.purpose.into(),
                destination: c.host.unwrap_or_default(),
                route: "custom_endpoint".into(),
                synthetic_only: self.synthetic_only,
                cost_micros: None,
                payload_text: Some(json!({"requestBytes":request_bytes}).to_string()),
            })
            .map_err(|_| GatewayError::Unavailable)?;
        Ok(format!("{id}:{request_bytes}"))
    }
    fn completed(
        &self,
        reference: &str,
        response_bytes: usize,
        usage: &Usage,
    ) -> Result<(), GatewayError> {
        let (id, request_bytes) = reference.split_once(':').ok_or(GatewayError::Unavailable)?;
        self.core.db().set_outbound_metadata(id, &json!({"requestBytes":request_bytes.parse::<usize>().unwrap_or(0),"responseBytes":response_bytes,"usage":usage}).to_string()).map_err(|_| GatewayError::Unavailable)
    }
}
