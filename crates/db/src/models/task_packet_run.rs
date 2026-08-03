use chrono::{DateTime, Utc};
use executors::profile::ExecutorConfig;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskPacketRepositoryMapping {
    pub logical_id: String,
    pub repo_id: Uuid,
    pub role: String,
    pub target_branch: String,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub forbidden_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskPacketProjectSettings {
    pub remote_project_id: Uuid,
    pub local_project_id: Option<Uuid>,
    pub enabled: bool,
    pub profile: String,
    pub executor_config: ExecutorConfig,
    pub in_progress_status_id: Option<Uuid>,
    pub review_status_id: Option<Uuid>,
    pub repository_mappings: Vec<TaskPacketRepositoryMapping>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpsertTaskPacketProjectSettings {
    pub local_project_id: Option<Uuid>,
    pub enabled: bool,
    pub profile: String,
    pub executor_config: ExecutorConfig,
    pub in_progress_status_id: Option<Uuid>,
    pub review_status_id: Option<Uuid>,
    pub repository_mappings: Vec<TaskPacketRepositoryMapping>,
}

#[derive(Debug, FromRow)]
struct SettingsRow {
    remote_project_id: Uuid,
    local_project_id: Option<Uuid>,
    enabled: bool,
    profile: String,
    executor_config: String,
    in_progress_status_id: Option<Uuid>,
    review_status_id: Option<Uuid>,
    repository_mappings: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<SettingsRow> for TaskPacketProjectSettings {
    type Error = serde_json::Error;

    fn try_from(row: SettingsRow) -> Result<Self, Self::Error> {
        Ok(Self {
            remote_project_id: row.remote_project_id,
            local_project_id: row.local_project_id,
            enabled: row.enabled,
            profile: row.profile,
            executor_config: serde_json::from_str(&row.executor_config)?,
            in_progress_status_id: row.in_progress_status_id,
            review_status_id: row.review_status_id,
            repository_mappings: serde_json::from_str(&row.repository_mappings)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TaskPacketProjectSettings {
    pub async fn find(
        pool: &SqlitePool,
        remote_project_id: Uuid,
    ) -> Result<Option<Self>, anyhow::Error> {
        let row = sqlx::query_as::<_, SettingsRow>(
            r#"SELECT remote_project_id, local_project_id, enabled, profile,
                      executor_config, in_progress_status_id, review_status_id,
                      repository_mappings, created_at, updated_at
               FROM task_packet_project_settings WHERE remote_project_id = ?"#,
        )
        .bind(remote_project_id)
        .fetch_optional(pool)
        .await?;
        row.map(TryInto::try_into).transpose().map_err(Into::into)
    }

    pub async fn upsert(
        pool: &SqlitePool,
        remote_project_id: Uuid,
        value: &UpsertTaskPacketProjectSettings,
    ) -> Result<Self, anyhow::Error> {
        let executor_config = serde_json::to_string(&value.executor_config)?;
        let repository_mappings = serde_json::to_string(&value.repository_mappings)?;
        sqlx::query(
            r#"INSERT INTO task_packet_project_settings (
                    remote_project_id, local_project_id, enabled, profile, executor_config,
                    in_progress_status_id, review_status_id, repository_mappings
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(remote_project_id) DO UPDATE SET
                    local_project_id = excluded.local_project_id,
                    enabled = excluded.enabled,
                    profile = excluded.profile,
                    executor_config = excluded.executor_config,
                    in_progress_status_id = excluded.in_progress_status_id,
                    review_status_id = excluded.review_status_id,
                    repository_mappings = excluded.repository_mappings,
                    updated_at = CURRENT_TIMESTAMP"#,
        )
        .bind(remote_project_id)
        .bind(value.local_project_id)
        .bind(value.enabled)
        .bind(&value.profile)
        .bind(executor_config)
        .bind(value.in_progress_status_id)
        .bind(value.review_status_id)
        .bind(repository_mappings)
        .execute(pool)
        .await?;
        Self::find(pool, remote_project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task Packet project settings were not persisted"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, FromRow)]
pub struct TaskPacketParentRun {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub remote_project_id: Uuid,
    pub revision: i64,
    pub state: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, FromRow)]
pub struct TaskPacketRun {
    pub id: Uuid,
    pub parent_run_id: Uuid,
    pub task_packet_id: Uuid,
    pub attempt: i64,
    pub state: String,
    pub workspace_id: Option<Uuid>,
    pub execution_process_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TaskPacketParentRun {
    pub async fn create(
        pool: &SqlitePool,
        issue_id: Uuid,
        remote_project_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        let revision: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM task_packet_parent_runs WHERE issue_id = ?",
        )
        .bind(issue_id)
        .fetch_one(pool)
        .await?;
        sqlx::query(
            "INSERT INTO task_packet_parent_runs (id, issue_id, remote_project_id, revision, state) VALUES (?, ?, ?, ?, 'compiling')",
        )
        .bind(id)
        .bind(issue_id)
        .bind(remote_project_id)
        .bind(revision)
        .execute(pool)
        .await?;
        Self::find(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn find(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT id, issue_id, remote_project_id, revision, state, error, created_at, updated_at FROM task_packet_parent_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_for_issue(
        pool: &SqlitePool,
        issue_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT id, issue_id, remote_project_id, revision, state, error, created_at, updated_at FROM task_packet_parent_runs WHERE issue_id = ? ORDER BY revision DESC",
        )
        .bind(issue_id)
        .fetch_all(pool)
        .await
    }

    pub async fn set_state(
        &self,
        pool: &SqlitePool,
        state: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE task_packet_parent_runs SET state = ?, error = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(state).bind(error).bind(self.id).execute(pool).await?;
        Ok(())
    }
}

impl TaskPacketRun {
    pub async fn create(
        pool: &SqlitePool,
        parent_run_id: Uuid,
        task_packet_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO task_packet_runs (id, parent_run_id, task_packet_id, state) VALUES (?, ?, ?, 'ready')")
            .bind(id).bind(parent_run_id).bind(task_packet_id).execute(pool).await?;
        Self::find(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn find(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT id, parent_run_id, task_packet_id, attempt, state, workspace_id, execution_process_id, created_at, updated_at FROM task_packet_runs WHERE id = ?",
        )
        .bind(id).fetch_optional(pool).await
    }

    pub async fn list_for_parent(
        pool: &SqlitePool,
        parent_run_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT id, parent_run_id, task_packet_id, attempt, state, workspace_id, execution_process_id, created_at, updated_at FROM task_packet_runs WHERE parent_run_id = ? ORDER BY attempt",
        )
        .bind(parent_run_id).fetch_all(pool).await
    }

    pub async fn mark_running(
        &self,
        pool: &SqlitePool,
        workspace_id: Uuid,
        execution_process_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE task_packet_runs SET state = 'running', workspace_id = ?, execution_process_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(workspace_id).bind(execution_process_id).bind(self.id).execute(pool).await?;
        Ok(())
    }

    pub async fn set_state(&self, pool: &SqlitePool, state: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE task_packet_runs SET state = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(state)
        .bind(self.id)
        .execute(pool)
        .await?;
        Ok(())
    }
}
