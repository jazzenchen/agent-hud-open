-- A made-up Goose session database: the tables Goose v1.57.0 creates for a
-- new database (schema 17, crates/goose/src/session/session_manager.rs) and
-- invented rows. Nothing here comes from a real machine.
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '', user_set_name BOOLEAN DEFAULT FALSE, session_type TEXT NOT NULL DEFAULT 'user', working_dir TEXT NOT NULL, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, extension_data TEXT DEFAULT '{}', total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER, cache_write_tokens INTEGER, accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER, accumulated_output_tokens INTEGER, accumulated_cache_read_tokens INTEGER, accumulated_cache_write_tokens INTEGER, accumulated_cost REAL, schedule_id TEXT, recipe_json TEXT, user_recipe_values_json TEXT, provider_name TEXT, model_config_json TEXT, goose_mode TEXT NOT NULL DEFAULT 'auto', archived_at TIMESTAMP, project_id TEXT, parent_session_id TEXT, system_prompt_override TEXT, system_prompt_extras_json TEXT, container_id TEXT);
CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY AUTOINCREMENT, message_id TEXT, session_id TEXT NOT NULL REFERENCES sessions(id), role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL, timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP, tokens INTEGER, metadata_json TEXT);
CREATE TABLE IF NOT EXISTS usage_ledger (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, created_timestamp INTEGER NOT NULL, model TEXT, input_tokens INTEGER, output_tokens INTEGER, total_tokens INTEGER, cache_read_tokens INTEGER, cache_write_tokens INTEGER, cost REAL, cost_source TEXT, is_compaction INTEGER DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_ledger_session ON usage_ledger(session_id);
INSERT INTO schema_version (version) VALUES (17);

INSERT INTO sessions (id, name, working_dir, created_at, updated_at, provider_name, model_config_json, accumulated_input_tokens, accumulated_output_tokens)
VALUES ('20261104_1', 'Fix the parser', '/Users/me/projects/app', '2026-10-26 07:33:20', '2026-10-26 07:38:20', 'anthropic', '{"model_name":"claude-sonnet-4-5","context_limit":200000}', 13000, 1200);
INSERT INTO sessions (id, name, session_type, working_dir, created_at, updated_at, provider_name, model_config_json, parent_session_id)
VALUES ('20261104_2', 'Look for the tests', 'sub_agent', '/Users/me/projects/app', '2026-10-26 07:35:00', '2026-10-26 07:36:40', 'openai', '{"model_name":"gpt-5.4"}', '20261104_1');
INSERT INTO sessions (id, description, working_dir, created_at, updated_at, provider_name, model_config_json, archived_at)
VALUES ('20261104_3', 'Old notes', '/Users/me/notes', '2026-10-26 07:33:20', '2026-10-26 07:34:10', 'ollama', '{"model_name":"llama3.2"}', '2026-10-26 07:34:10');

INSERT INTO usage_ledger (session_id, created_timestamp, model, input_tokens, output_tokens, total_tokens, cache_read_tokens, cache_write_tokens, cost, cost_source, is_compaction)
VALUES ('20261104_1', 1793000100, 'claude-sonnet-4-5', 10000, 300, 10300, 8000, 800, 0.0123, 'provider_reported', 0);
INSERT INTO usage_ledger (session_id, created_timestamp, model, input_tokens, output_tokens, total_tokens, cache_read_tokens, cache_write_tokens, cost, cost_source, is_compaction)
VALUES ('20261104_1', 1793000200, 'claude-sonnet-4-5', 3000, 900, 3900, 0, 0, 0.0225, 'estimated', 1);
INSERT INTO usage_ledger (session_id, created_timestamp, model, input_tokens, output_tokens, total_tokens, cache_read_tokens, cache_write_tokens, cost, cost_source, is_compaction)
VALUES ('20261104_1', 1793000200, NULL, 4000, 100, 4100, 0, 0, NULL, 'carried_forward', 0);
INSERT INTO usage_ledger (session_id, created_timestamp, model, input_tokens, output_tokens, total_tokens, cache_read_tokens, cache_write_tokens, cost, cost_source, is_compaction)
VALUES ('20261104_2', 1793000150, 'gpt-5.5', 500, 50, 550, 0, 0, 0.001, 'estimated', 0);
INSERT INTO usage_ledger (session_id, created_timestamp, model, input_tokens, output_tokens, total_tokens, cache_read_tokens, cache_write_tokens, cost, cost_source, is_compaction)
VALUES ('20261104_2', 1793000160, NULL, 600, 60, 660, 100, 0, NULL, NULL, 0);
