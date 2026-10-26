use macroscope_builder::*;
use macroscope_domain::{CsvMapping, TaskContract};
use macroscope_model_gateway::{GatewayError, MetaResponsesClient};
use macroscope_muse_client::{CredentialRoute, PlanStatus};
use macroscope_policy::EgressPolicy;
use macroscope_vault::{MemoryStore, Secret, SecretRef, SecretStore};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
fn contract() -> TaskContract {
    serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap()
}
struct Validator;
impl CandidateValidator for Validator {
    fn validate(
        &self,
        contract: &TaskContract,
        mapping: &CsvMapping,
    ) -> Result<(), ValidationError> {
        // Independent host checks, deliberately separate from schema validation.
        if mapping
            .output_columns
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>()
            != contract.output_columns.iter().collect::<Vec<_>>()
            || !mapping.formula_escape
            || mapping
                .output_columns
                .iter()
                .any(|c| !contract.input_schema.contains(&c.source))
        {
            return Err(ValidationError);
        }
        Ok(())
    }
}
fn response(value: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"status":"completed","output":[{"content":[{"type":"output_text","text":serde_json::to_string(&value).unwrap()}]}],"usage":{"input_tokens":100,"output_tokens":200}}))
}
#[tokio::test]
async fn direct_meta_fixture_synthetic_packet_and_host_validation() {
    let c = contract();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(response(serde_json::to_value(&c.mapping).unwrap()))
        .expect(1)
        .mount(&server)
        .await;
    let vault = MemoryStore::default();
    let key = SecretRef::new("meta-api-key").unwrap();
    vault
        .set(&key, Secret::new(b"bogus-builder-canary".to_vec()))
        .unwrap();
    let policy = EgressPolicy::default();
    let epoch = policy.allow_cloud().unwrap();
    let builder = MappingBuilder {
        route: BuildRoute::DirectMeta { key_ref: key },
        vault: &vault,
        gateway: MetaResponsesClient::new(server.uri()),
        policy: &policy,
        consent_epoch: epoch,
        muse: None,
    };
    let result = builder
        .propose(
            &c,
            SyntheticSamples::from_contract(&c, 20),
            BuildBudget::default(),
            &Validator,
        )
        .await
        .unwrap();
    assert_eq!(result.mapping, c.mapping);
    assert_eq!(result.cost_microusd, Some(975));
    assert_eq!(result.reserved_microusd, 0);
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let packet: Value =
        serde_json::from_str(body["input"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        packet["synthetic_example_rows"].as_array().unwrap().len(),
        5
    );
    assert_eq!(
        packet["untrusted_contract_data"]["columns"],
        json!(c.input_schema)
    );
    assert!(packet.get("csv_mapping_json_schema").is_some());
    assert!(!String::from_utf8_lossy(&requests[0].body).contains("bogus-builder-canary"));
}
#[tokio::test]
async fn malformed_then_corrected_once_and_unknown_spend_stays_reserved() {
    let c = contract();
    let good = serde_json::to_value(&c.mapping).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let counter = count.clone();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |_: &wiremock::Request| {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                response(json!({"invalid":true}))
            } else {
                response(good.clone())
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let vault = MemoryStore::default();
    let key = SecretRef::new("meta-api-key").unwrap();
    vault
        .set(&key, Secret::new(b"bogus-canary".to_vec()))
        .unwrap();
    let policy = EgressPolicy::default();
    let epoch = policy.allow_cloud().unwrap();
    let builder = MappingBuilder {
        route: BuildRoute::DirectMeta { key_ref: key },
        vault: &vault,
        gateway: MetaResponsesClient::new(server.uri()),
        policy: &policy,
        consent_epoch: epoch,
        muse: None,
    };
    let result = builder
        .propose(
            &c,
            SyntheticSamples::from_contract(&c, 5),
            BuildBudget::default(),
            &Validator,
        )
        .await
        .unwrap();
    assert_eq!(result.mapping, c.mapping);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(result.reserved_microusd > 0);
    assert!(result.cost_microusd.is_none());
    let requests = server.received_requests().await.unwrap();
    assert!(String::from_utf8_lossy(&requests[1].body).contains("Schema correction"));
}
#[tokio::test]
async fn malformed_twice_fails_no_third_dispatch() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(response(json!({"invalid":true})))
        .expect(2)
        .mount(&server)
        .await;
    let c = contract();
    let vault = MemoryStore::default();
    let key = SecretRef::new("meta-api-key").unwrap();
    vault
        .set(&key, Secret::new(b"bogus-canary".to_vec()))
        .unwrap();
    let p = EgressPolicy::default();
    let e = p.allow_cloud().unwrap();
    let b = MappingBuilder {
        route: BuildRoute::DirectMeta { key_ref: key },
        vault: &vault,
        gateway: MetaResponsesClient::new(server.uri()),
        policy: &p,
        consent_epoch: e,
        muse: None,
    };
    assert!(matches!(
        b.propose(
            &c,
            SyntheticSamples::from_contract(&c, 5),
            BuildBudget::default(),
            &Validator
        )
        .await,
        Err(BuildError::Gateway(GatewayError::MalformedOutput))
    ));
}
#[tokio::test]
async fn budget_exhaustion_and_stale_epoch_prevent_dispatch() {
    let server = MockServer::start().await;
    let c = contract();
    let vault = MemoryStore::default();
    let key = SecretRef::new("meta-api-key").unwrap();
    let p = EgressPolicy::default();
    let e = p.allow_cloud().unwrap();
    let b = MappingBuilder {
        route: BuildRoute::DirectMeta { key_ref: key },
        vault: &vault,
        gateway: MetaResponsesClient::new(server.uri()),
        policy: &p,
        consent_epoch: e,
        muse: None,
    };
    assert!(matches!(
        b.propose(
            &c,
            SyntheticSamples::from_contract(&c, 5),
            BuildBudget {
                max_cost_microusd: 1,
                ..Default::default()
            },
            &Validator
        )
        .await,
        Err(BuildError::BudgetExceeded)
    ));
    p.begin_local_only().unwrap().wait().await.unwrap();
    assert!(matches!(
        b.propose(
            &c,
            SyntheticSamples::from_contract(&c, 5),
            BuildBudget::default(),
            &Validator
        )
        .await,
        Err(BuildError::EgressDenied)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}
#[tokio::test]
async fn offline_is_labelled_and_validator_can_reject() {
    let mut c = contract();
    let vault = MemoryStore::default();
    let p = EgressPolicy::default();
    let b = MappingBuilder {
        route: BuildRoute::Offline,
        vault: &vault,
        gateway: MetaResponsesClient::default(),
        policy: &p,
        consent_epoch: 0,
        muse: None,
    };
    let result = b
        .propose(
            &c,
            SyntheticSamples::from_contract(&c, 5),
            BuildBudget::default(),
            &Validator,
        )
        .await
        .unwrap();
    assert_eq!(result.mapping, c.mapping);
    assert_eq!(result.label, "Offline deterministic heuristic");
    assert_eq!(result.cost_microusd, Some(0));
    c.mapping.formula_escape = false;
    assert!(matches!(
        b.propose(
            &c,
            SyntheticSamples::from_contract(&c, 5),
            BuildBudget::default(),
            &Validator
        )
        .await,
        Err(BuildError::Validation)
    ));
}
#[test]
fn route_selection_prioritizes_key_then_plan_and_types_no_access() {
    let key = SecretRef::new("meta-api-key").unwrap();
    let signed = PlanStatus::SignedIn {
        summary: json!({"plan":"pro"}),
    };
    assert_eq!(
        select_route(Some(key.clone()), &signed, ApiPreference::DirectMeta).unwrap(),
        BuildRoute::DirectMeta {
            key_ref: key.clone()
        }
    );
    assert_eq!(
        select_route(Some(key), &PlanStatus::NotSignedIn, ApiPreference::MuseExec).unwrap(),
        BuildRoute::MuseExec {
            route: CredentialRoute::ApiKey
        }
    );
    assert_eq!(
        select_route(None, &signed, ApiPreference::DirectMeta).unwrap(),
        BuildRoute::MuseExec {
            route: CredentialRoute::PlanSignIn
        }
    );
    assert!(matches!(
        select_route(None, &PlanStatus::NotSignedIn, ApiPreference::DirectMeta),
        Err(BuildError::NoModelAccess(NoAccessReason::NotSignedIn))
    ));
    assert!(matches!(
        select_route(None, &PlanStatus::MuseMissing, ApiPreference::DirectMeta),
        Err(BuildError::NoModelAccess(NoAccessReason::MuseMissing))
    ));
}

#[tokio::test]
async fn rejected_api_key_revokes_admission_and_never_falls_back_to_plan() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let contract = contract();
    let vault = MemoryStore::default();
    let key = SecretRef::new("meta-api-key").unwrap();
    vault
        .set(&key, Secret::new(b"bogus-canary".to_vec()))
        .unwrap();
    let policy = EgressPolicy::default();
    let epoch = policy.allow_cloud().unwrap();
    let builder = MappingBuilder {
        route: BuildRoute::DirectMeta { key_ref: key },
        vault: &vault,
        gateway: MetaResponsesClient::new(server.uri()),
        policy: &policy,
        consent_epoch: epoch,
        muse: None,
    };
    assert!(matches!(
        builder
            .propose(
                &contract,
                SyntheticSamples::from_contract(&contract, 5),
                BuildBudget::default(),
                &Validator
            )
            .await,
        Err(BuildError::Gateway(GatewayError::AuthRejected))
    ));
    assert_eq!(
        policy.state(),
        macroscope_policy::EgressState::KeyUnavailable
    );
    assert!(matches!(
        builder
            .propose(
                &contract,
                SyntheticSamples::from_contract(&contract, 5),
                BuildBudget::default(),
                &Validator
            )
            .await,
        Err(BuildError::EgressDenied)
    ));
}
