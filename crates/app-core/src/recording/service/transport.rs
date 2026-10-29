//! Multimodal adapter using the same bounded, redirect-free gateway HTTP client as builds.
//! No file staging, tools, fallback routes, retries or provider diagnostics.
use super::*;
use crate::{db, new_id, MetaResponsesClient};

pub(super) async fn analyze(
    core: &AppCore,
    request: &ModelRequest,
    preview: &RecordingPreview,
    route: ModelRoute,
) -> CoreResult<TaskDraftV2> {
    let schema: serde_json::Value =
        serde_json::from_str(request.response_schema_json()).map_err(err)?;
    let mut content =
        vec![serde_json::json!({"type":"input_text","text":request.untrusted_data_json()})];
    for frame in request.frames() {
        content.push(serde_json::json!({"type":"input_image","image_url":format!("data:image/png;base64,{}",encode(&frame.png))}));
    }
    let (url, key, body) = match route {
        ModelRoute::ApiKey => {
            let key = core
                .inner
                .cfg
                .vault
                .get(&AppCore::api_key_ref())
                .map_err(|_| invalid("Your model key is unavailable."))?
                .ok_or_else(|| invalid("Add a model key in Settings."))?;
            let body = serde_json::json!({"model":preview.model,"store":false,"max_output_tokens":request.max_output_tokens(),
                "input":[{"role":"system","content":request.system()},{"role":"user","content":content}],
                "text":{"format":{"type":"json_schema","name":"task_draft_v2","strict":true,"schema":schema}}});
            (
                format!(
                    "{}/responses",
                    core.inner.cfg.gateway_base_url.trim_end_matches('/')
                ),
                key,
                body,
            )
        }
        ModelRoute::CustomEndpoint => {
            let c = core.custom_endpoint_status();
            let base = c
                .base_url
                .ok_or_else(|| invalid("Configure a custom endpoint in Settings."))?;
            macroscope_model_gateway::validate_custom_url(&base, core.allow_insecure_web())
                .map_err(err)?;
            let key = core
                .inner
                .cfg
                .vault
                .get(&AppCore::custom_token_ref())
                .map_err(|_| invalid("Your endpoint token is unavailable."))?
                .ok_or_else(|| invalid("Add an endpoint token in Settings."))?;
            let content: Vec<_> = content
                .into_iter()
                .map(|p| {
                    if p["type"] == "input_text" {
                        serde_json::json!({"type":"text","text":p["text"]})
                    } else {
                        serde_json::json!({"type":"image_url","image_url":{"url":p["image_url"]}})
                    }
                })
                .collect();
            let body = serde_json::json!({"model":preview.model,"max_tokens":request.max_output_tokens(),
                "messages":[{"role":"system","content":request.system()},{"role":"user","content":content}],
                "response_format":{"type":"json_schema","json_schema":{"name":"task_draft_v2","strict":true,"schema":schema}}});
            (
                format!("{}/chat/completions", base.trim_end_matches('/')),
                key,
                body,
            )
        }
        _ => return Err(invalid("Choose an image-capable API model in Settings.")),
    };
    let bytes = serde_json::to_vec(&body).map_err(err)?;
    let mut header = b"Bearer ".to_vec();
    header.extend_from_slice(key.expose());
    // HeaderValue is inferred from the gateway's existing reqwest client; prevent diagnostics from escaping.
    let client = MetaResponsesClient::new(&core.inner.cfg.gateway_base_url);
    let builder = client
        .http
        .post(url)
        .header("authorization", header)
        .header("content-type", "application/json");
    if route == ModelRoute::CustomEndpoint {
        let reference = core
            .custom_observer("Recording analysis", preview.sample)
            .dispatch(bytes.len())
            .map_err(err)?;
        let id = reference
            .split_once(':')
            .ok_or_else(|| invalid("Could not record the outbound request."))?
            .0;
        core.db().set_outbound_metadata(id, &preview.payload_text)?;
    } else {
        core.db().insert_outbound(&db::OutboundRow {
            id: new_id("recording_out"),
            at_ms: core.now(),
            purpose: "Recording analysis".into(),
            destination: preview.recipient.clone(),
            route: route.as_str().into(),
            synthetic_only: preview.sample,
            cost_micros: None,
            payload_text: Some(preview.payload_text.clone()),
        })?;
    }
    let mut response = builder.body(bytes).send().await.map_err(|_| {
        invalid("The model couldn't be reached. Discard this recording and try again.")
    })?;
    if !response.status().is_success() {
        return Err(invalid(
            "The model couldn't analyze this recording. Check your model settings and try again.",
        ));
    }
    let mut response_bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| invalid("The model response was interrupted."))?
    {
        if chunk.len() > (1024 * 1024usize).saturating_sub(response_bytes.len()) {
            return Err(invalid("The model response was too large."));
        }
        response_bytes.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&response_bytes)
        .map_err(|_| invalid("The model returned an unreadable response."))?;
    let text = if route == ModelRoute::CustomEndpoint {
        if value["choices"][0]["finish_reason"] == "length" {
            return Err(invalid("The model response was cut short."));
        }
        value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string()
    } else {
        if value["status"] == "incomplete"
            || value
                .get("incomplete_details")
                .is_some_and(|v| !v.is_null())
        {
            return Err(invalid("The model response was cut short."));
        }
        value["output"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|i| i["content"].as_array().into_iter().flatten())
            .filter(|p| p["type"] == "output_text")
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("")
    };
    if !key.expose().is_empty()
        && text
            .as_bytes()
            .windows(key.expose().len())
            .any(|w| w == key.expose())
    {
        return Err(invalid("The model response couldn't be used."));
    }
    request.parse_draft(&text).map_err(err)
}
