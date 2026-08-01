use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use task_packet_protocol::{
    ProtocolError, document_sha256, validate_result_envelope as validate_result_schema,
    validate_task_packet as validate_task_packet_schema,
};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

pub const SUPPORTED_TASK_PACKET_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Error)]
pub enum TaskPacketError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("Invalid Task Packet Protocol document: {0}")]
    InvalidDocument(String),
    #[error("Task packet not found")]
    NotFound,
    #[error("A task packet with packet_id '{0}' already exists")]
    DuplicatePacketId(String),
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateTaskPacket {
    pub issue_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub execution_process_id: Option<Uuid>,
    #[ts(type = "unknown")]
    pub packet: Value,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateTaskPacketResult {
    pub execution_process_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    #[ts(type = "unknown")]
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskPacket {
    pub id: Uuid,
    pub packet_id: String,
    pub task_id: String,
    pub issue_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub execution_process_id: Option<Uuid>,
    pub schema_version: i64,
    pub payload_sha256: String,
    #[ts(type = "unknown")]
    pub packet: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskPacketResult {
    pub id: Uuid,
    pub task_packet_id: Uuid,
    pub execution_process_id: Option<Uuid>,
    pub schema_version: i64,
    pub status: String,
    pub payload_sha256: String,
    #[ts(type = "unknown")]
    pub result: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct TaskPacketRow {
    id: Uuid,
    packet_id: String,
    task_id: String,
    issue_id: Option<Uuid>,
    workspace_id: Option<Uuid>,
    execution_process_id: Option<Uuid>,
    schema_version: i64,
    payload: String,
    payload_sha256: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct TaskPacketResultRow {
    id: Uuid,
    task_packet_id: Uuid,
    execution_process_id: Option<Uuid>,
    schema_version: i64,
    status: String,
    payload: String,
    payload_sha256: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn required_string(document: &Value, field: &str) -> Result<String, TaskPacketError> {
    document
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            TaskPacketError::InvalidDocument(format!("'{field}' must be a non-empty string"))
        })
}

fn validate_schema_version(document: &Value) -> Result<i64, TaskPacketError> {
    let version = document
        .get("schema_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            TaskPacketError::InvalidDocument("'schema_version' must be an integer".to_string())
        })?;

    if version != SUPPORTED_TASK_PACKET_SCHEMA_VERSION {
        return Err(TaskPacketError::InvalidDocument(format!(
            "unsupported schema_version {version}; supported version is {SUPPORTED_TASK_PACKET_SCHEMA_VERSION}"
        )));
    }

    Ok(version)
}

fn validate_task_packet(document: &Value) -> Result<(i64, String, String), TaskPacketError> {
    validate_task_packet_schema(document)?;
    let version = validate_schema_version(document)?;
    let packet_id = required_string(document, "packet_id")?;
    let task_id = required_string(document, "task_id")?;
    required_string(document, "title")?;
    required_string(document, "objective")?;
    Ok((version, packet_id, task_id))
}

fn validate_result_envelope(
    document: &Value,
    expected_packet_id: &str,
    expected_task_id: &str,
) -> Result<(i64, String), TaskPacketError> {
    validate_result_schema(document)?;
    let version = validate_schema_version(document)?;
    let packet_id = required_string(document, "packet_id")?;
    let task_id = required_string(document, "task_id")?;
    let status = required_string(document, "status")?;

    if packet_id != expected_packet_id {
        return Err(TaskPacketError::InvalidDocument(format!(
            "result packet_id '{packet_id}' does not match '{expected_packet_id}'"
        )));
    }
    if task_id != expected_task_id {
        return Err(TaskPacketError::InvalidDocument(format!(
            "result task_id '{task_id}' does not match '{expected_task_id}'"
        )));
    }
    if !matches!(status.as_str(), "complete" | "blocked" | "failed") {
        return Err(TaskPacketError::InvalidDocument(format!(
            "unsupported result status '{status}'"
        )));
    }

    Ok((version, status))
}

impl TryFrom<TaskPacketRow> for TaskPacket {
    type Error = TaskPacketError;

    fn try_from(row: TaskPacketRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            packet_id: row.packet_id,
            task_id: row.task_id,
            issue_id: row.issue_id,
            workspace_id: row.workspace_id,
            execution_process_id: row.execution_process_id,
            schema_version: row.schema_version,
            payload_sha256: row.payload_sha256,
            packet: serde_json::from_str(&row.payload)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TryFrom<TaskPacketResultRow> for TaskPacketResult {
    type Error = TaskPacketError;

    fn try_from(row: TaskPacketResultRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            task_packet_id: row.task_packet_id,
            execution_process_id: row.execution_process_id,
            schema_version: row.schema_version,
            status: row.status,
            payload_sha256: row.payload_sha256,
            result: serde_json::from_str(&row.payload)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TaskPacket {
    pub async fn create(
        pool: &SqlitePool,
        request: &CreateTaskPacket,
    ) -> Result<Self, TaskPacketError> {
        let (schema_version, packet_id, task_id) = validate_task_packet(&request.packet)?;

        if Self::find_by_packet_id(pool, &packet_id).await?.is_some() {
            return Err(TaskPacketError::DuplicatePacketId(packet_id));
        }

        let id = Uuid::new_v4();
        let payload = serde_json::to_string(&request.packet)?;
        let payload_sha256 = document_sha256(&request.packet)?;
        sqlx::query(
            r#"INSERT INTO task_packets (
                id, packet_id, task_id, issue_id, workspace_id,
                execution_process_id, schema_version, payload, payload_sha256
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(&packet_id)
        .bind(&task_id)
        .bind(request.issue_id)
        .bind(request.workspace_id)
        .bind(request.execution_process_id)
        .bind(schema_version)
        .bind(payload)
        .bind(payload_sha256)
        .execute(pool)
        .await?;

        Self::find_by_id(pool, id)
            .await?
            .ok_or(TaskPacketError::NotFound)
    }

    pub async fn find_all(pool: &SqlitePool) -> Result<Vec<Self>, TaskPacketError> {
        let rows = sqlx::query_as::<_, TaskPacketRow>(
            r#"SELECT id, packet_id, task_id, issue_id, workspace_id,
                      execution_process_id, schema_version, payload, payload_sha256,
                      created_at, updated_at
               FROM task_packets
               ORDER BY created_at DESC"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, TaskPacketError> {
        let row = sqlx::query_as::<_, TaskPacketRow>(
            r#"SELECT id, packet_id, task_id, issue_id, workspace_id,
                      execution_process_id, schema_version, payload, payload_sha256,
                      created_at, updated_at
               FROM task_packets WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn find_by_packet_id(
        pool: &SqlitePool,
        packet_id: &str,
    ) -> Result<Option<Self>, TaskPacketError> {
        let row = sqlx::query_as::<_, TaskPacketRow>(
            r#"SELECT id, packet_id, task_id, issue_id, workspace_id,
                      execution_process_id, schema_version, payload, payload_sha256,
                      created_at, updated_at
               FROM task_packets WHERE packet_id = ?"#,
        )
        .bind(packet_id)
        .fetch_optional(pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn create_result(
        &self,
        pool: &SqlitePool,
        request: &CreateTaskPacketResult,
    ) -> Result<TaskPacketResult, TaskPacketError> {
        let (schema_version, status) =
            validate_result_envelope(&request.result, &self.packet_id, &self.task_id)?;
        let id = Uuid::new_v4();
        let payload = serde_json::to_string(&request.result)?;
        let payload_sha256 = document_sha256(&request.result)?;

        sqlx::query(
            r#"INSERT INTO task_packet_results (
                id, task_packet_id, execution_process_id,
                schema_version, status, payload, payload_sha256
            ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(self.id)
        .bind(request.execution_process_id)
        .bind(schema_version)
        .bind(status)
        .bind(payload)
        .bind(payload_sha256)
        .execute(pool)
        .await?;

        TaskPacketResult::find_by_id(pool, id)
            .await?
            .ok_or(TaskPacketError::NotFound)
    }

    pub async fn create_result_for_run(
        &self,
        pool: &SqlitePool,
        packet_run_id: Uuid,
        request: &CreateTaskPacketResult,
    ) -> Result<TaskPacketResult, TaskPacketError> {
        let (schema_version, status) =
            validate_result_envelope(&request.result, &self.packet_id, &self.task_id)?;
        let id = Uuid::new_v4();
        let payload = serde_json::to_string(&request.result)?;
        let payload_sha256 = document_sha256(&request.result)?;
        let inserted = sqlx::query(
            r#"INSERT INTO task_packet_results (
                id, task_packet_id, packet_run_id, execution_process_id,
                schema_version, status, payload, payload_sha256
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(packet_run_id) DO NOTHING"#,
        )
        .bind(id)
        .bind(self.id)
        .bind(packet_run_id)
        .bind(request.execution_process_id)
        .bind(schema_version)
        .bind(status)
        .bind(payload)
        .bind(payload_sha256)
        .execute(pool)
        .await?;
        if inserted.rows_affected() == 0 {
            return Err(TaskPacketError::InvalidDocument(
                "a result has already been submitted for this packet run".to_string(),
            ));
        }
        TaskPacketResult::find_by_id(pool, id)
            .await?
            .ok_or(TaskPacketError::NotFound)
    }
}

impl TaskPacketResult {
    pub async fn find_by_packet_run_id(
        pool: &SqlitePool,
        packet_run_id: Uuid,
    ) -> Result<Option<Self>, TaskPacketError> {
        let row = sqlx::query_as::<_, TaskPacketResultRow>(
            r#"SELECT id, task_packet_id, execution_process_id,
                      schema_version, status, payload, payload_sha256, created_at, updated_at
               FROM task_packet_results WHERE packet_run_id = ?"#,
        )
        .bind(packet_run_id)
        .fetch_optional(pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, TaskPacketError> {
        let row = sqlx::query_as::<_, TaskPacketResultRow>(
            r#"SELECT id, task_packet_id, execution_process_id,
                      schema_version, status, payload, payload_sha256, created_at, updated_at
               FROM task_packet_results WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn find_by_task_packet_id(
        pool: &SqlitePool,
        task_packet_id: Uuid,
    ) -> Result<Vec<Self>, TaskPacketError> {
        let rows = sqlx::query_as::<_, TaskPacketResultRow>(
            r#"SELECT id, task_packet_id, execution_process_id,
                      schema_version, status, payload, payload_sha256, created_at, updated_at
               FROM task_packet_results
               WHERE task_packet_id = ?
               ORDER BY created_at ASC"#,
        )
        .bind(task_packet_id)
        .fetch_all(pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{validate_result_envelope, validate_task_packet};

    fn task_packet() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../task-packet-protocol/fixtures/task-packet.example.json"
        ))
        .unwrap()
    }

    #[test]
    fn accepts_protocol_v1_task_packet_shape() {
        let result = validate_task_packet(&task_packet()).unwrap();
        assert_eq!(
            result,
            (
                1,
                "weather-042-client".to_string(),
                "weather-042-client".to_string()
            )
        );
    }

    #[test]
    fn rejects_unknown_protocol_version() {
        let mut packet = task_packet();
        packet["schema_version"] = json!(2);
        assert!(validate_task_packet(&packet).is_err());
    }

    #[test]
    fn rejects_result_for_another_packet() {
        let mut result: serde_json::Value = serde_json::from_str(include_str!(
            "../../../task-packet-protocol/fixtures/result-envelope.example.json"
        ))
        .unwrap();
        result["packet_id"] = json!("other-packet");
        assert!(
            validate_result_envelope(&result, "weather-042-client", "weather-042-client").is_err()
        );
    }
}
