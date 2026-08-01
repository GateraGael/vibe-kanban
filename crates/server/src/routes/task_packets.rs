use axum::{
    Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::task_packet::{
    CreateTaskPacket, CreateTaskPacketResult, SUPPORTED_TASK_PACKET_SCHEMA_VERSION, TaskPacket,
    TaskPacketError, TaskPacketResult,
};
use deployment::Deployment;
use serde::Serialize;
use serde_json::{Value, json};
use task_packet_protocol::validate_adapter_manifest;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Serialize, TS)]
pub struct TaskPacketAdapterManifest {
    pub schema_version: u64,
    pub adapter_id: &'static str,
    pub adapter_kind: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub protocol_versions: Vec<u64>,
    pub capabilities: Vec<String>,
    #[ts(type = "unknown")]
    pub configuration: Value,
    #[ts(type = "unknown | null")]
    pub metadata: Option<Value>,
}

async fn get_adapter_manifest()
-> Result<ResponseJson<ApiResponse<TaskPacketAdapterManifest>>, ApiError> {
    let manifest = TaskPacketAdapterManifest {
        schema_version: SUPPORTED_TASK_PACKET_SCHEMA_VERSION as u64,
        adapter_id: "vibe-kanban",
        adapter_kind: "orchestrator",
        display_name: "Vibe Kanban",
        version: env!("CARGO_PKG_VERSION"),
        protocol_versions: vec![SUPPORTED_TASK_PACKET_SCHEMA_VERSION as u64],
        capabilities: vec![
            "persist_task_packets".to_string(),
            "link_issues".to_string(),
            "link_workspaces".to_string(),
            "link_executions".to_string(),
            "persist_result_envelopes".to_string(),
            "validate_protocol_documents".to_string(),
        ],
        configuration: json!({
            "task_packet_mode": false,
            "full_schema_validation": true
        }),
        metadata: Some(json!({
            "status": "persistence_api_v1"
        })),
    };
    let value =
        serde_json::to_value(&manifest).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    validate_adapter_manifest(&value).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    Ok(ResponseJson(ApiResponse::success(manifest)))
}

impl From<TaskPacketError> for ApiError {
    fn from(error: TaskPacketError) -> Self {
        match error {
            TaskPacketError::Database(error) => ApiError::Database(error),
            TaskPacketError::Serde(error) => ApiError::BadRequest(error.to_string()),
            TaskPacketError::Protocol(error) => ApiError::BadRequest(error.to_string()),
            TaskPacketError::InvalidDocument(message) => ApiError::BadRequest(message),
            TaskPacketError::NotFound => ApiError::BadRequest("Task packet not found".to_string()),
            TaskPacketError::DuplicatePacketId(packet_id) => {
                ApiError::Conflict(format!("Task packet '{packet_id}' already exists"))
            }
        }
    }
}

async fn list_task_packets(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskPacket>>>, ApiError> {
    let packets = TaskPacket::find_all(&deployment.db().pool).await?;
    Ok(ResponseJson(ApiResponse::success(packets)))
}

async fn create_task_packet(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateTaskPacket>,
) -> Result<ResponseJson<ApiResponse<TaskPacket>>, ApiError> {
    let packet = TaskPacket::create(&deployment.db().pool, &request).await?;
    Ok(ResponseJson(ApiResponse::success(packet)))
}

async fn get_task_packet(
    State(deployment): State<DeploymentImpl>,
    Path(task_packet_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<TaskPacket>>, ApiError> {
    let packet = TaskPacket::find_by_id(&deployment.db().pool, task_packet_id)
        .await?
        .ok_or(TaskPacketError::NotFound)?;
    Ok(ResponseJson(ApiResponse::success(packet)))
}

async fn list_task_packet_results(
    State(deployment): State<DeploymentImpl>,
    Path(task_packet_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskPacketResult>>>, ApiError> {
    TaskPacket::find_by_id(&deployment.db().pool, task_packet_id)
        .await?
        .ok_or(TaskPacketError::NotFound)?;
    let results =
        TaskPacketResult::find_by_task_packet_id(&deployment.db().pool, task_packet_id).await?;
    Ok(ResponseJson(ApiResponse::success(results)))
}

async fn create_task_packet_result(
    State(deployment): State<DeploymentImpl>,
    Path(task_packet_id): Path<Uuid>,
    Json(request): Json<CreateTaskPacketResult>,
) -> Result<ResponseJson<ApiResponse<TaskPacketResult>>, ApiError> {
    let packet = TaskPacket::find_by_id(&deployment.db().pool, task_packet_id)
        .await?
        .ok_or(TaskPacketError::NotFound)?;
    let result = packet
        .create_result(&deployment.db().pool, &request)
        .await?;
    Ok(ResponseJson(ApiResponse::success(result)))
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new()
        .route("/task-packets/adapter-manifest", get(get_adapter_manifest))
        .route(
            "/task-packets",
            get(list_task_packets).post(create_task_packet),
        )
        .route("/task-packets/{task_packet_id}", get(get_task_packet))
        .route(
            "/task-packets/{task_packet_id}/results",
            get(list_task_packet_results).post(create_task_packet_result),
        )
}
