CREATE TABLE task_packet_project_settings (
    remote_project_id BLOB PRIMARY KEY NOT NULL,
    local_project_id BLOB NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    profile TEXT NOT NULL,
    executor_config TEXT NOT NULL,
    in_progress_status_id BLOB,
    review_status_id BLOB,
    repository_mappings TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (local_project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE task_packet_parent_runs (
    id BLOB PRIMARY KEY NOT NULL,
    issue_id BLOB NOT NULL,
    remote_project_id BLOB NOT NULL,
    revision INTEGER NOT NULL,
    state TEXT NOT NULL,
    error TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (issue_id, revision),
    CHECK (state IN ('compiling', 'running', 'review', 'complete', 'blocked', 'failed', 'cancelled'))
);

CREATE INDEX idx_task_packet_parent_runs_issue_id
    ON task_packet_parent_runs(issue_id, revision DESC);

CREATE TABLE task_packet_runs (
    id BLOB PRIMARY KEY NOT NULL,
    parent_run_id BLOB NOT NULL,
    task_packet_id BLOB NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 1,
    state TEXT NOT NULL,
    workspace_id BLOB,
    execution_process_id BLOB,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (parent_run_id) REFERENCES task_packet_parent_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (task_packet_id) REFERENCES task_packets(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE SET NULL,
    UNIQUE (parent_run_id, attempt),
    CHECK (state IN ('ready', 'running', 'validating', 'complete', 'blocked', 'failed', 'cancelled'))
);

CREATE INDEX idx_task_packet_runs_parent_run_id ON task_packet_runs(parent_run_id);
CREATE INDEX idx_task_packet_runs_execution_process_id ON task_packet_runs(execution_process_id);

ALTER TABLE task_packet_results
    ADD COLUMN packet_run_id BLOB REFERENCES task_packet_runs(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX idx_task_packet_results_packet_run_id
    ON task_packet_results(packet_run_id);
