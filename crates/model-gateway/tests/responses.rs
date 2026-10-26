use macroscope_model_gateway::*;
use macroscope_vault::Secret;
use serde_json::{json, Value};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};
fn schema() -> Value {
    json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false})
}
fn response(text: &str) -> Value {
    json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":text}]}],"usage":{"input_tokens":100,"output_tokens":20,"input_tokens_details":{"cached_tokens":10},"output_tokens_details":{"reasoning_tokens":5}}})
}
async fn call(client: &MetaResponsesClient) -> Result<StructuredOutput, GatewayError> {
    let key = Secret::new(b"bogus-gateway-canary-1234".to_vec());
    let schema = schema();
    let req = StructuredRequest {
        key: &key,
        model: None,
        system: "Use schema",
        user: "synthetic",
        max_output_tokens: 100,
        reasoning_effort: "low",
        schema_name: "answer",
        schema: &schema,
    };
    assert!(!format!("{req:?}").contains("bogus-gateway-canary"));
    client.generate_structured(req).await
}
#[tokio::test]
async fn responses_text_format_shape_store_false_and_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(header("authorization", "Bearer bogus-gateway-canary-1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response(r#"{"ok":true}"#)))
        .expect(1)
        .mount(&server)
        .await;
    let out = call(&MetaResponsesClient::new(server.uri())).await.unwrap();
    assert_eq!(out.value, json!({"ok":true}));
    assert_eq!(
        out.usage,
        Usage {
            input_tokens: 100,
            cached_tokens: 10,
            output_tokens: 20,
            reasoning_tokens: 5
        }
    );
    assert!(out.usage_known);
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["text"]["format"],
        json!({"type":"json_schema","name":"answer","schema":schema(),"strict":true})
    );
    assert!(body.get("response_format").is_none());
    assert_eq!(body["store"], false);
    assert_eq!(body["model"], DEFAULT_MODEL);
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["input"][0]["role"], "system");
    assert_eq!(body["input"][1]["role"], "user");
}
#[tokio::test]
async fn error_classification_and_secret_never_in_errors_or_debug() {
    for (status, error) in [
        (401, GatewayError::AuthRejected),
        (403, GatewayError::AuthRejected),
        (402, GatewayError::BillingRequired),
        (
            429,
            GatewayError::Quota {
                retry_after: Some(std::time::Duration::from_secs(17)),
            },
        ),
        (413, GatewayError::PayloadTooLarge),
        (503, GatewayError::Transient),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("Retry-After", "17")
                    .set_body_string("bogus-gateway-canary-1234".repeat(1000)),
            )
            .mount(&server)
            .await;
        let e = call(&MetaResponsesClient::new(server.uri()))
            .await
            .unwrap_err();
        assert_eq!(e, error);
        assert!(!format!("{e} {e:?}").contains("bogus-gateway-canary-1234"));
        assert!(e.to_string().len() <= 500);
    }
}
#[tokio::test]
async fn incomplete_refusal_malformed_json_and_schema_mismatch() {
    let mut incomplete = response(r#"{"ok":true}"#);
    incomplete["status"] = "incomplete".into();
    let mut details = response(r#"{"ok":true}"#);
    details["incomplete_details"] = json!({"reason":"max_output_tokens"});
    let refusal =
        json!({"output":[{"content":[{"type":"refusal","refusal":"bogus-gateway-canary-1234"}]}]});
    for (body, error) in [
        (incomplete, GatewayError::Truncated),
        (details, GatewayError::Truncated),
        (refusal, GatewayError::Refused),
        (
            response("bad JSON bogus-gateway-canary-1234"),
            GatewayError::MalformedOutput,
        ),
        (response(r#"{"ok":"wrong"}"#), GatewayError::MalformedOutput),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let e = call(&MetaResponsesClient::new(server.uri()))
            .await
            .unwrap_err();
        assert_eq!(e, error);
        assert!(!format!("{e:?}").contains("bogus-gateway-canary-1234"));
    }
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("invalid HTTP JSON"))
        .mount(&server)
        .await;
    assert_eq!(
        call(&MetaResponsesClient::new(server.uri()))
            .await
            .unwrap_err(),
        GatewayError::MalformedOutput
    );
}
#[tokio::test]
async fn catalog_is_read_only_and_typed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":[{"id":DEFAULT_MODEL}]})),
        )
        .mount(&server)
        .await;
    let catalog = MetaResponsesClient::new(server.uri())
        .catalog_check(&Secret::new(b"bogus-canary".to_vec()))
        .await
        .unwrap();
    assert_eq!(catalog.models, [DEFAULT_MODEL]);
}
#[test]
fn integer_microdollar_prices_no_reasoning_double_count_and_round_up() {
    let usage = Usage {
        input_tokens: 1_000_000,
        cached_tokens: 200_000,
        output_tokens: 100_000,
        reasoning_tokens: 20_000,
    };
    assert_eq!(
        CostEstimator::default()
            .estimate(DEFAULT_MODEL, &usage)
            .unwrap(),
        1_455_000
    );
    assert_eq!(
        CostEstimator {
            reasoning_in_output: false
        }
        .estimate(DEFAULT_MODEL, &usage)
        .unwrap(),
        1_540_000
    );
    assert_eq!(
        CostEstimator::default()
            .estimate("muse-wonder-1.0", &usage)
            .unwrap(),
        5_240_000
    );
    assert_eq!(
        CostEstimator::default()
            .estimate(
                DEFAULT_MODEL,
                &Usage {
                    input_tokens: 1,
                    ..Default::default()
                }
            )
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn network_failure_is_transient_and_success_cannot_echo_key_in_debug() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let disconnect = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        drop(socket);
        drop(listener);
    });
    assert_eq!(
        call(&MetaResponsesClient::new(url)).await.unwrap_err(),
        GatewayError::Transient
    );
    disconnect.await.unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response(
            r#"{"ok":true,"secret":"bogus-gateway-canary-1234"}"#,
        )))
        .mount(&server)
        .await;
    let error = call(&MetaResponsesClient::new(server.uri()))
        .await
        .unwrap_err();
    assert_eq!(error, GatewayError::MalformedOutput);
    assert!(!format!("{error:?}").contains("bogus-gateway-canary-1234"));
}
