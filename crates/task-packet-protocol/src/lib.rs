use std::{collections::BTreeMap, sync::OnceLock};

use jsonschema::{Registry, Resource, Validator};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u64 = 1;
const SCHEMA_BASE_URI: &str = "https://task-packet.local/schemas/";

const ADAPTER_MANIFEST_SCHEMA: &str = include_str!("../schemas/adapter-manifest.schema.json");
const CONTEXT_SLICE_SCHEMA: &str = include_str!("../schemas/context-slice.schema.json");
const EXECUTION_PROVENANCE_SCHEMA: &str =
    include_str!("../schemas/execution-provenance.schema.json");
const INTEGRATION_LEDGER_SCHEMA: &str = include_str!("../schemas/integration-ledger.schema.json");
const RESULT_ENVELOPE_SCHEMA: &str = include_str!("../schemas/result-envelope.schema.json");
const TASK_PACKET_SCHEMA: &str = include_str!("../schemas/task-packet.schema.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    AdapterManifest,
    IntegrationLedger,
    ResultEnvelope,
    TaskPacket,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Protocol schema setup failed: {0}")]
    Schema(String),
    #[error("Invalid {kind}: {details}")]
    Validation { kind: &'static str, details: String },
    #[error("Canonical JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub schema_version: u64,
    pub adapter_id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub version: String,
    pub protocol_versions: Vec<u64>,
    pub capabilities: Vec<String>,
    pub configuration: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

struct Validators {
    adapter_manifest: Validator,
    integration_ledger: Validator,
    result_envelope: Validator,
    task_packet: Validator,
}

static VALIDATORS: OnceLock<Result<Validators, String>> = OnceLock::new();

fn parse_schema(name: &str, contents: &str) -> Result<Value, String> {
    serde_json::from_str(contents).map_err(|error| format!("{name}: {error}"))
}

fn build_validator(schema: &Value, registry: &Registry<'_>) -> Result<Validator, String> {
    jsonschema::draft202012::options()
        .with_base_uri(SCHEMA_BASE_URI)
        .with_registry(registry)
        .should_validate_formats(true)
        .build(schema)
        .map_err(|error| error.to_string())
}

fn build_validators() -> Result<Validators, String> {
    let adapter_manifest = parse_schema("adapter manifest", ADAPTER_MANIFEST_SCHEMA)?;
    let context_slice = parse_schema("context slice", CONTEXT_SLICE_SCHEMA)?;
    let execution_provenance = parse_schema("execution provenance", EXECUTION_PROVENANCE_SCHEMA)?;
    let integration_ledger = parse_schema("integration ledger", INTEGRATION_LEDGER_SCHEMA)?;
    let result_envelope = parse_schema("result envelope", RESULT_ENVELOPE_SCHEMA)?;
    let task_packet = parse_schema("task packet", TASK_PACKET_SCHEMA)?;

    let registry = Registry::new()
        .add(
            format!("{SCHEMA_BASE_URI}context-slice.schema.json"),
            Resource::from_contents(context_slice),
        )
        .map_err(|error| error.to_string())?
        .add(
            format!("{SCHEMA_BASE_URI}execution-provenance.schema.json"),
            Resource::from_contents(execution_provenance),
        )
        .map_err(|error| error.to_string())?
        .prepare()
        .map_err(|error| error.to_string())?;

    Ok(Validators {
        adapter_manifest: build_validator(&adapter_manifest, &registry)?,
        integration_ledger: build_validator(&integration_ledger, &registry)?,
        result_envelope: build_validator(&result_envelope, &registry)?,
        task_packet: build_validator(&task_packet, &registry)?,
    })
}

fn validators() -> Result<&'static Validators, ProtocolError> {
    VALIDATORS
        .get_or_init(build_validators)
        .as_ref()
        .map_err(|error| ProtocolError::Schema(error.clone()))
}

fn validator_for(kind: DocumentKind) -> Result<&'static Validator, ProtocolError> {
    let validators = validators()?;
    Ok(match kind {
        DocumentKind::AdapterManifest => &validators.adapter_manifest,
        DocumentKind::IntegrationLedger => &validators.integration_ledger,
        DocumentKind::ResultEnvelope => &validators.result_envelope,
        DocumentKind::TaskPacket => &validators.task_packet,
    })
}

fn kind_name(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::AdapterManifest => "adapter manifest",
        DocumentKind::IntegrationLedger => "integration ledger",
        DocumentKind::ResultEnvelope => "result envelope",
        DocumentKind::TaskPacket => "task packet",
    }
}

pub fn validate(kind: DocumentKind, document: &Value) -> Result<(), ProtocolError> {
    let validator = validator_for(kind)?;
    let errors = validator
        .iter_errors(document)
        .take(8)
        .map(|error| {
            let path = error.instance_path().to_string();
            if path.is_empty() {
                error.to_string()
            } else {
                format!("{path}: {error}")
            }
        })
        .collect::<Vec<_>>();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ProtocolError::Validation {
            kind: kind_name(kind),
            details: errors.join("; "),
        })
    }
}

pub fn validate_task_packet(document: &Value) -> Result<(), ProtocolError> {
    validate(DocumentKind::TaskPacket, document)
}

pub fn validate_result_envelope(document: &Value) -> Result<(), ProtocolError> {
    validate(DocumentKind::ResultEnvelope, document)
}

pub fn validate_integration_ledger(document: &Value) -> Result<(), ProtocolError> {
    validate(DocumentKind::IntegrationLedger, document)
}

pub fn validate_adapter_manifest(document: &Value) -> Result<(), ProtocolError> {
    validate(DocumentKind::AdapterManifest, document)
}

fn canonical_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonical_value).collect()),
        Value::Object(values) => {
            let sorted = values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_value(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(Map::from_iter(sorted))
        }
        _ => value.clone(),
    }
}

pub fn canonical_json(document: &Value) -> Result<String, ProtocolError> {
    Ok(serde_json::to_string(&canonical_value(document))?)
}

pub fn document_sha256(document: &Value) -> Result<String, ProtocolError> {
    let canonical = canonical_json(document)?;
    Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        DocumentKind, canonical_json, document_sha256, validate, validate_adapter_manifest,
        validate_result_envelope, validate_task_packet,
    };

    fn fixture(contents: &str) -> Value {
        serde_json::from_str(contents).unwrap()
    }

    #[test]
    fn validates_protocol_examples() {
        validate_task_packet(&fixture(include_str!(
            "../fixtures/task-packet.example.json"
        )))
        .unwrap();
        validate_result_envelope(&fixture(include_str!(
            "../fixtures/result-envelope.example.json"
        )))
        .unwrap();
        validate(
            DocumentKind::IntegrationLedger,
            &fixture(include_str!("../fixtures/integration-ledger.example.json")),
        )
        .unwrap();
        validate_adapter_manifest(&fixture(include_str!(
            "../fixtures/adapter-manifest.example.json"
        )))
        .unwrap();
    }

    #[test]
    fn rejects_nested_schema_violations() {
        let mut packet = fixture(include_str!("../fixtures/task-packet.example.json"));
        packet["scope"]["write"] = json!([{"repository": "application"}]);
        assert!(validate_task_packet(&packet).is_err());
    }

    #[test]
    fn canonical_digest_ignores_object_key_order() {
        let left = json!({"b": 2, "a": {"d": 4, "c": 3}});
        let right = json!({"a": {"c": 3, "d": 4}, "b": 2});
        assert_eq!(
            canonical_json(&left).unwrap(),
            canonical_json(&right).unwrap()
        );
        assert_eq!(
            document_sha256(&left).unwrap(),
            document_sha256(&right).unwrap()
        );
    }
}
