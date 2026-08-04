use rmcp::{
    ErrorData, handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool,
    tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::McpServer;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompileIssueToPacketRequest {
    #[schemars(description = "Remote issue ID to compile and dispatch")]
    issue_id: Uuid,
    #[schemars(description = "Optional Task Packet kind; defaults to implementation")]
    kind: Option<String>,
    #[schemars(description = "Optional explicit acceptance checks")]
    acceptance: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetParentRunRequest {
    parent_run_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SubmitTaskPacketResultRequest {
    packet_run_id: Uuid,
    #[schemars(description = "A complete Task Packet Protocol Result Envelope")]
    result: Value,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TaskPacketToolResponse {
    data: Value,
}

fn parse_result_document(result: Value) -> Result<Value, serde_json::Error> {
    match result {
        Value::String(document) => serde_json::from_str(&document),
        document => Ok(document),
    }
}

#[tool_router(router = task_packets_tools_router, vis = "pub")]
impl McpServer {
    #[tool(
        description = "Compile one Vibe issue into a validated Task Packet and dispatch it to a local Vibe agent execution"
    )]
    async fn compile_issue_to_packets(
        &self,
        Parameters(request): Parameters<CompileIssueToPacketRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!(
            "/api/issues/{}/task-packet-runs",
            request.issue_id
        ));
        let payload = serde_json::json!({"kind": request.kind, "acceptance": request.acceptance});
        let response: Value = match self.send_json(self.client.post(&url).json(&payload)).await {
            Ok(value) => value,
            Err(error) => return Ok(McpServer::tool_error(error)),
        };
        McpServer::success(&TaskPacketToolResponse { data: response })
    }

    #[tool(
        description = "Get a Task Packet parent run, its local execution, and any submitted result"
    )]
    async fn get_parent_run(
        &self,
        Parameters(request): Parameters<GetParentRunRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-packet-runs/{}", request.parent_run_id));
        let response: Value = match self.send_json(self.client.get(&url)).await {
            Ok(value) => value,
            Err(error) => return Ok(McpServer::tool_error(error)),
        };
        McpServer::success(&TaskPacketToolResponse { data: response })
    }

    #[tool(description = "Submit the single schema-valid Result Envelope for a Task Packet run")]
    async fn submit_task_packet_result(
        &self,
        Parameters(request): Parameters<SubmitTaskPacketResultRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let workspace_id = match self.scoped_workspace_id() {
            Some(workspace_id) => workspace_id,
            None => {
                return McpServer::err(
                    "Task Packet result submission requires workspace context",
                    None,
                );
            }
        };
        let result = match parse_result_document(request.result) {
            Ok(result) => result,
            Err(error) => {
                return McpServer::err(
                    "The Result Envelope JSON string is invalid".to_string(),
                    Some(error.to_string()),
                );
            }
        };
        let url = self.url(&format!(
            "/api/packet-runs/{}/result",
            request.packet_run_id
        ));
        let payload = serde_json::json!({
            "execution_process_id": null,
            "workspace_id": workspace_id,
            "result": result
        });
        let response: Value = match self.send_json(self.client.post(&url).json(&payload)).await {
            Ok(value) => value,
            Err(error) => return Ok(McpServer::tool_error(error)),
        };
        McpServer::success(&TaskPacketToolResponse { data: response })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_result_document;

    #[test]
    fn accepts_object_result_document() {
        let document = json!({"schema_version": 1});
        assert_eq!(parse_result_document(document.clone()).unwrap(), document);
    }

    #[test]
    fn accepts_json_string_result_document() {
        assert_eq!(
            parse_result_document(json!(r#"{"schema_version":1}"#)).unwrap(),
            json!({"schema_version": 1})
        );
    }

    #[test]
    fn rejects_invalid_json_string_result_document() {
        assert!(parse_result_document(json!("not-json")).is_err());
    }
}
