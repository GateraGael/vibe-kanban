CREATE TABLE task_packets (
    id BLOB PRIMARY KEY NOT NULL,
    packet_id TEXT NOT NULL UNIQUE,
    task_id TEXT NOT NULL,
    issue_id BLOB,
    workspace_id BLOB,
    execution_process_id BLOB,
    schema_version INTEGER NOT NULL,
    payload TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE SET NULL,
    CHECK (schema_version = 1)
);

CREATE INDEX idx_task_packets_issue_id ON task_packets(issue_id);
CREATE INDEX idx_task_packets_workspace_id ON task_packets(workspace_id);
CREATE INDEX idx_task_packets_execution_process_id
    ON task_packets(execution_process_id);

CREATE TABLE task_packet_results (
    id BLOB PRIMARY KEY NOT NULL,
    task_packet_id BLOB NOT NULL,
    execution_process_id BLOB,
    schema_version INTEGER NOT NULL,
    status TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (task_packet_id) REFERENCES task_packets(id) ON DELETE CASCADE,
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE SET NULL,
    CHECK (schema_version = 1),
    CHECK (status IN ('complete', 'blocked', 'failed'))
);

CREATE INDEX idx_task_packet_results_packet_id
    ON task_packet_results(task_packet_id);
CREATE INDEX idx_task_packet_results_execution_process_id
    ON task_packet_results(execution_process_id);
