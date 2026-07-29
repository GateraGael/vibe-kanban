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
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Serialize, TS)]
pub struct TaskPacketAdapterManifest {
    pub adapter: &'static str,
    pub adapter_version: &'static str,
    pub protocol_versions: Vec<i64>,
    pub capabilities: Vec<&'static str>,
}

async fn get_adapter_manifest() -> ResponseJson<ApiResponse<TaskPacketAdapterManifest>> {
    ResponseJson(ApiResponse::success(TaskPacketAdapterManifest {
        adapter: "vibe-kanban",
        adapter_version: env!("CARGO_PKG_VERSION"),
        protocol_versions: vec![SUPPORTED_TASK_PACKET_SCHEMA_VERSION],
        capabilities: vec![
            "persist_task_packets",
            "link_issues",
            "link_workspaces",
            "link_executions",
            "persist_result_envelopes",
        ],
    }))
}

impl From<TaskPacketError> for ApiError {
    fn from(error: TaskPacketError) -> Self {
        match error {
            TaskPacketError::Database(error) => ApiError::Database(error),
            TaskPacketError::Serde(error) => ApiError::BadRequest(error.to_string()),
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
