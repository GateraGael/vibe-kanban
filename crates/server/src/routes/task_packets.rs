use api_types::UpdateIssueRequest;
use axum::{
    Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::{
    requests::{CreateAndStartWorkspaceRequest, LinkedIssueInfo, WorkspaceRepoInput},
    task_packet::{
        CreateTaskPacket, CreateTaskPacketResult, SUPPORTED_TASK_PACKET_SCHEMA_VERSION, TaskPacket,
        TaskPacketError, TaskPacketResult,
    },
    task_packet_run::{
        TaskPacketParentRun, TaskPacketProjectSettings, TaskPacketRun,
        UpsertTaskPacketProjectSettings,
    },
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use task_packet_protocol::validate_adapter_manifest;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    DeploymentImpl, error::ApiError, routes::workspaces::create::create_and_start_workspace_inner,
};

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
            "compile_single_packet".to_string(),
            "dispatch_local_execution".to_string(),
            "ingest_result_via_mcp".to_string(),
            "project_board_state".to_string(),
        ],
        configuration: json!({
            "task_packet_mode": "project-configurable",
            "full_schema_validation": true,
            "max_packets_per_parent_run": 1,
            "transport": "local-vibe-execution"
        }),
        metadata: Some(json!({
            "status": "headless_single_packet_v1"
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

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CompileAndDispatchTaskPacket {
    pub kind: Option<String>,
    pub acceptance: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct TaskPacketRunDetails {
    pub parent_run: TaskPacketParentRun,
    pub packet_runs: Vec<TaskPacketRun>,
    pub packet: Option<TaskPacket>,
    pub result: Option<TaskPacketResult>,
}

async fn get_project_settings(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<TaskPacketProjectSettings>>, ApiError> {
    let settings = TaskPacketProjectSettings::find(&deployment.db().pool, project_id)
        .await
        .map_err(|error| ApiError::BadRequest(error.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("Task Packet Mode is not configured".to_string()))?;
    Ok(ResponseJson(ApiResponse::success(settings)))
}

async fn put_project_settings(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<UpsertTaskPacketProjectSettings>,
) -> Result<ResponseJson<ApiResponse<TaskPacketProjectSettings>>, ApiError> {
    if request.profile.trim().is_empty() || request.repository_mappings.is_empty() {
        return Err(ApiError::BadRequest(
            "A profile and at least one repository mapping are required".to_string(),
        ));
    }
    for mapping in &request.repository_mappings {
        if mapping.logical_id.trim().is_empty()
            || !matches!(mapping.role.as_str(), "primary" | "supporting")
            || mapping.target_branch.trim().is_empty()
        {
            return Err(ApiError::BadRequest(format!(
                "Invalid repository mapping for '{}'",
                mapping.logical_id
            )));
        }
    }
    let settings = TaskPacketProjectSettings::upsert(&deployment.db().pool, project_id, &request)
        .await
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    Ok(ResponseJson(ApiResponse::success(settings)))
}

fn compile_packet(
    issue: &api_types::Issue,
    settings: &TaskPacketProjectSettings,
    revision: i64,
    request: &CompileAndDispatchTaskPacket,
) -> Value {
    let packet_id = format!("{}-r{}", issue.simple_id, revision);
    let objective = issue
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&issue.title);
    let repositories = settings
        .repository_mappings
        .iter()
        .map(|mapping| json!({"id": mapping.logical_id, "role": mapping.role, "root": null}))
        .collect::<Vec<_>>();
    let resources =
        |paths: fn(&db::models::task_packet_run::TaskPacketRepositoryMapping) -> &Vec<String>| {
            settings
                .repository_mappings
                .iter()
                .flat_map(|mapping| {
                    paths(mapping)
                        .iter()
                        .map(|path| json!({"repository": mapping.logical_id, "path": path}))
                })
                .collect::<Vec<_>>()
        };
    let acceptance = request.acceptance.clone().unwrap_or_else(|| {
        vec![
            "Complete the issue objective and run relevant project validation.".to_string(),
            "Return one schema-valid Result Envelope through submit_task_packet_result."
                .to_string(),
        ]
    });
    json!({
        "schema_version": 1,
        "packet_id": packet_id,
        "task_id": issue.id.to_string(),
        "parent_task_id": null,
        "title": issue.title,
        "objective": objective,
        "kind": request.kind.as_deref().unwrap_or("implementation"),
        "project": {"id": settings.profile, "repositories": repositories},
        "stack": settings.profile,
        "parallel_group": null,
        "scope": {
            "read": resources(|mapping| &mapping.read_paths),
            "write": resources(|mapping| &mapping.write_paths),
            "forbidden": resources(|mapping| &mapping.forbidden_paths)
        },
        "agent_requirements": {
            "required_capabilities": ["read_files", "write_files", "structured_output"],
            "preferred_capabilities": ["shell", "test_execution"],
            "constraints": {"transport": "local-vibe-execution"}
        },
        "context_budget_tokens": 4096,
        "context_slices": [],
        "dependencies": [],
        "deliverables": ["Issue objective completed", "Schema-valid Result Envelope"],
        "acceptance": acceptance.into_iter().enumerate().map(|(index, check)| json!({
            "id": format!("acceptance-{}", index + 1), "type": "condition", "check": check, "required": true
        })).collect::<Vec<_>>(),
        "result_consumers": ["vibe-kanban-review"]
    })
}

fn execution_prompt(packet_run_id: Uuid, packet: &Value) -> Result<String, ApiError> {
    let packet_id = packet["packet_id"]
        .as_str()
        .unwrap_or("copy-from-task-packet");
    let task_id = packet["task_id"]
        .as_str()
        .unwrap_or("copy-from-task-packet");
    let packet = serde_json::to_string_pretty(packet)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    Ok(format!(
        "Execute the following immutable Task Packet exactly within its declared scope. Follow repository instructions, perform the acceptance checks, and do not expand permissions. When finished, call the Vibe MCP tool `submit_task_packet_result` exactly once with packet_run_id `{packet_run_id}` and a schema-valid Result Envelope. Do not treat a prose summary as submission. Use the Result Envelope shape below and replace every placeholder with accurate data.\n\nTask Packet:\n```json\n{packet}\n```\n\nResult Envelope shape:\n```json\n{{\n  \"schema_version\": 1,\n  \"packet_id\": \"{packet_id}\",\n  \"task_id\": \"{task_id}\",\n  \"execution\": {{\n    \"orchestrator\": {{\"adapter_id\": \"vibe-kanban\", \"run_id\": \"{packet_run_id}\", \"host\": \"local\", \"workspace_ref\": \"current-workspace\", \"session_ref\": \"current-session\"}},\n    \"agent\": {{\"adapter_id\": \"vibe-local-agent\", \"agent_name\": \"current-agent\", \"model_provider\": \"current-provider\", \"model\": \"current-model\", \"agent_session_ref\": \"current-agent-session\"}},\n    \"started_at\": \"RFC3339 timestamp\",\n    \"ended_at\": \"RFC3339 timestamp\",\n    \"metadata\": {{\"transport\": \"local-vibe-execution\"}}\n  }},\n  \"status\": \"complete|blocked|failed\",\n  \"summary\": \"concise outcome\",\n  \"changes\": [{{\"repository\": \"logical repository id\", \"path\": \"changed/path\", \"purpose\": \"why it changed\"}}],\n  \"contracts\": {{\"provided\": {{}}, \"changed\": []}},\n  \"validation\": [{{\"check\": \"acceptance check\", \"result\": \"passed|failed|blocked|not_run\", \"details\": \"evidence\"}}],\n  \"decisions\": [],\n  \"risks\": [],\n  \"artifacts\": [],\n  \"context_used\": [],\n  \"context_requested\": []\n}}\n```"
    ))
}

async fn project_issue_status(
    deployment: &DeploymentImpl,
    issue_id: Uuid,
    status_id: Option<Uuid>,
) -> Result<(), ApiError> {
    let Some(status_id) = status_id else {
        return Ok(());
    };
    deployment
        .remote_client()?
        .update_issue(
            issue_id,
            &UpdateIssueRequest {
                status_id: Some(status_id),
                title: None,
                description: None,
                priority: None,
                start_date: None,
                target_date: None,
                completed_at: None,
                sort_order: None,
                parent_issue_id: None,
                parent_issue_sort_order: None,
                extension_metadata: None,
            },
        )
        .await?;
    Ok(())
}

async fn compile_and_dispatch(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
    Json(request): Json<CompileAndDispatchTaskPacket>,
) -> Result<ResponseJson<ApiResponse<TaskPacketRunDetails>>, ApiError> {
    let client = deployment.remote_client()?;
    let issue = client.get_issue(issue_id).await?;
    let settings = TaskPacketProjectSettings::find(&deployment.db().pool, issue.project_id)
        .await
        .map_err(|error| ApiError::BadRequest(error.to_string()))?
        .ok_or_else(|| {
            ApiError::BadRequest("Task Packet Mode is not configured for this project".to_string())
        })?;
    if !settings.enabled {
        return Err(ApiError::BadRequest(
            "Task Packet Mode is disabled for this project".to_string(),
        ));
    }

    let parent_run =
        TaskPacketParentRun::create(&deployment.db().pool, issue.id, issue.project_id).await?;
    let packet_value = compile_packet(&issue, &settings, parent_run.revision, &request);
    let packet = TaskPacket::create(
        &deployment.db().pool,
        &CreateTaskPacket {
            issue_id: Some(issue.id),
            workspace_id: None,
            execution_process_id: None,
            packet: packet_value,
        },
    )
    .await?;
    let packet_run = TaskPacketRun::create(&deployment.db().pool, parent_run.id, packet.id).await?;
    let repos = settings
        .repository_mappings
        .iter()
        .map(|mapping| WorkspaceRepoInput {
            repo_id: mapping.repo_id,
            target_branch: mapping.target_branch.clone(),
        })
        .collect();
    let response = create_and_start_workspace_inner(
        &deployment,
        CreateAndStartWorkspaceRequest {
            name: Some(format!(
                "{} Task Packet r{}",
                issue.simple_id, parent_run.revision
            )),
            repos,
            linked_issue: Some(LinkedIssueInfo {
                remote_project_id: issue.project_id,
                issue_id: issue.id,
            }),
            executor_config: settings.executor_config.clone(),
            prompt: execution_prompt(packet_run.id, &packet.packet)?,
            attachment_ids: None,
        },
    )
    .await;
    let response = match response {
        Ok(value) => value,
        Err(error) => {
            packet_run
                .set_state(&deployment.db().pool, "failed")
                .await?;
            parent_run
                .set_state(&deployment.db().pool, "failed", Some(&error.to_string()))
                .await?;
            return Err(error);
        }
    };
    packet_run
        .mark_running(
            &deployment.db().pool,
            response.workspace.id,
            response.execution_process.id,
        )
        .await?;
    parent_run
        .set_state(&deployment.db().pool, "running", None)
        .await?;
    project_issue_status(&deployment, issue.id, settings.in_progress_status_id).await?;
    let parent_run = TaskPacketParentRun::find(&deployment.db().pool, parent_run.id)
        .await?
        .unwrap();
    let packet_run = TaskPacketRun::find(&deployment.db().pool, packet_run.id)
        .await?
        .unwrap();
    Ok(ResponseJson(ApiResponse::success(TaskPacketRunDetails {
        parent_run,
        packet_runs: vec![packet_run],
        packet: Some(packet),
        result: None,
    })))
}

async fn list_issue_runs(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskPacketParentRun>>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(
        TaskPacketParentRun::list_for_issue(&deployment.db().pool, issue_id).await?,
    )))
}

async fn get_parent_run(
    State(deployment): State<DeploymentImpl>,
    Path(parent_run_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<TaskPacketRunDetails>>, ApiError> {
    let parent_run = TaskPacketParentRun::find(&deployment.db().pool, parent_run_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Task Packet parent run not found".to_string()))?;
    let packet_runs = TaskPacketRun::list_for_parent(&deployment.db().pool, parent_run_id).await?;
    let packet = match packet_runs.first() {
        Some(run) => TaskPacket::find_by_id(&deployment.db().pool, run.task_packet_id).await?,
        None => None,
    };
    let result = match packet_runs.first() {
        Some(run) => TaskPacketResult::find_by_packet_run_id(&deployment.db().pool, run.id).await?,
        None => None,
    };
    Ok(ResponseJson(ApiResponse::success(TaskPacketRunDetails {
        parent_run,
        packet_runs,
        packet,
        result,
    })))
}

async fn submit_packet_run_result(
    State(deployment): State<DeploymentImpl>,
    Path(packet_run_id): Path<Uuid>,
    Json(request): Json<CreateTaskPacketResult>,
) -> Result<ResponseJson<ApiResponse<TaskPacketResult>>, ApiError> {
    let run = TaskPacketRun::find(&deployment.db().pool, packet_run_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Task Packet run not found".to_string()))?;
    if request.workspace_id.is_none() || request.workspace_id != run.workspace_id {
        return Err(ApiError::Forbidden(
            "The result submission does not belong to this packet run's workspace".to_string(),
        ));
    }
    let reported_run_id = request
        .result
        .pointer("/execution/orchestrator/run_id")
        .and_then(Value::as_str);
    let expected_run_id = run.id.to_string();
    if reported_run_id != Some(expected_run_id.as_str()) {
        return Err(ApiError::BadRequest(
            "Result Envelope execution.orchestrator.run_id does not match the packet run"
                .to_string(),
        ));
    }
    let packet = TaskPacket::find_by_id(&deployment.db().pool, run.task_packet_id)
        .await?
        .ok_or(TaskPacketError::NotFound)?;
    run.set_state(&deployment.db().pool, "validating").await?;
    let result = packet
        .create_result_for_run(
            &deployment.db().pool,
            run.id,
            &CreateTaskPacketResult {
                execution_process_id: run.execution_process_id,
                workspace_id: run.workspace_id,
                result: request.result,
            },
        )
        .await?;
    let final_state = result.status.as_str();
    run.set_state(&deployment.db().pool, final_state).await?;
    let parent = TaskPacketParentRun::find(&deployment.db().pool, run.parent_run_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Task Packet parent run not found".to_string()))?;
    let parent_state = if final_state == "complete" {
        "review"
    } else {
        final_state
    };
    parent
        .set_state(&deployment.db().pool, parent_state, None)
        .await?;
    let settings = TaskPacketProjectSettings::find(&deployment.db().pool, parent.remote_project_id)
        .await
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    if final_state == "complete"
        && let Some(settings) = settings
    {
        project_issue_status(&deployment, parent.issue_id, settings.review_status_id).await?;
    }
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
        .route(
            "/projects/{project_id}/orchestration",
            get(get_project_settings).put(put_project_settings),
        )
        .route(
            "/issues/{issue_id}/task-packet-runs",
            get(list_issue_runs).post(compile_and_dispatch),
        )
        .route("/task-packet-runs/{parent_run_id}", get(get_parent_run))
        .route(
            "/packet-runs/{packet_run_id}/result",
            axum::routing::post(submit_packet_run_result),
        )
}

#[cfg(test)]
mod tests {
    use api_types::Issue;
    use db::models::task_packet_run::TaskPacketProjectSettings;
    use serde_json::json;
    use task_packet_protocol::validate_task_packet;
    use uuid::Uuid;

    use super::{CompileAndDispatchTaskPacket, compile_packet, execution_prompt};

    #[test]
    fn compiles_issue_to_protocol_valid_packet() {
        let project_id = Uuid::new_v4();
        let issue: Issue = serde_json::from_value(json!({
            "id": Uuid::new_v4(), "project_id": project_id, "issue_number": 7,
            "simple_id": "M2E-7", "status_id": Uuid::new_v4(), "title": "Test solar system",
            "description": "Validate the imported solar system asset.", "priority": "high",
            "start_date": null, "target_date": null, "completed_at": null, "sort_order": 0.0,
            "parent_issue_id": null, "parent_issue_sort_order": null, "extension_metadata": {},
            "creator_user_id": null, "created_at": "2026-08-01T00:00:00Z",
            "updated_at": "2026-08-01T00:00:00Z"
        }))
        .unwrap();
        let settings: TaskPacketProjectSettings = serde_json::from_value(json!({
            "remote_project_id": project_id, "local_project_id": Uuid::new_v4(), "enabled": true,
            "profile": "mission-to", "executor_config": {"executor": "CODEX"},
            "in_progress_status_id": null, "review_status_id": null,
            "repository_mappings": [{
                "logical_id": "mission-to", "repo_id": Uuid::new_v4(), "role": "primary",
                "target_branch": "dev", "read_paths": ["**"], "write_paths": ["Source/**"],
                "forbidden_paths": ["Saved/**"]
            }],
            "created_at": "2026-08-01T00:00:00Z", "updated_at": "2026-08-01T00:00:00Z"
        }))
        .unwrap();
        let packet = compile_packet(
            &issue,
            &settings,
            1,
            &CompileAndDispatchTaskPacket {
                kind: None,
                acceptance: None,
            },
        );
        validate_task_packet(&packet).unwrap();
        assert_eq!(packet["packet_id"], "M2E-7-r1");
        assert_eq!(packet["scope"]["write"][0]["repository"], "mission-to");
    }

    #[test]
    fn execution_prompt_requires_structured_mcp_submission() {
        let run_id = Uuid::new_v4();
        let prompt = execution_prompt(run_id, &json!({"packet_id": "packet-1"})).unwrap();
        assert!(prompt.contains("submit_task_packet_result"));
        assert!(prompt.contains(&run_id.to_string()));
    }
}
