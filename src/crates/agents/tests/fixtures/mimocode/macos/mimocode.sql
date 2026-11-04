-- A synthetic mimocode.db, shaped as MiMo Code 0.1 writes it
-- (XiaomiMiMo/MiMo-Code 6b66e8c, packages/cli/src/session/session.sql.ts and
-- external-import.sql.ts): a session of its own, still untitled; a Claude
-- Code session it imported and then went on in; and an older import that
-- named no messages.
CREATE TABLE session(
  id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, directory TEXT, title TEXT,
  title_source TEXT NOT NULL DEFAULT 'user', version TEXT, time_created INTEGER,
  time_updated INTEGER, time_archived INTEGER
);
CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, agent_id TEXT NOT NULL DEFAULT 'main', time_created INTEGER, data TEXT);
CREATE TABLE external_import(
  source TEXT NOT NULL, source_key TEXT NOT NULL, session_id TEXT NOT NULL, source_path TEXT NOT NULL,
  source_mtime INTEGER NOT NULL, time_imported INTEGER NOT NULL, message_ids TEXT,
  PRIMARY KEY(source, source_key)
);
INSERT INTO session VALUES('ses_own','prj',NULL,'/Users/me/work/app','Untitled','fallback','0.1.16',1792470000000,1792470030000,NULL);
INSERT INTO session VALUES('ses_cc','prj',NULL,'/Users/me/work/app','Imported from Claude Code','user','claude-code',1792400000000,1792470100000,NULL);
INSERT INTO session VALUES('ses_old','prj',NULL,'/Users/me/work/app','Imported earlier','user','claude-code',1792300000000,1792300000000,NULL);
INSERT INTO message VALUES('msg_own','ses_own','main',1792470001000,'{"role":"assistant","time":{"created":1792470001000,"completed":1792470030000},"modelID":"mimo-v2-pro","providerID":"xiaomi","tokens":{"input":800,"output":120,"reasoning":30,"cache":{"read":0,"write":0}},"finish":"stop"}');
INSERT INTO message VALUES('msg_sub','ses_own','general',1792470002000,'{"role":"assistant","time":{"created":1792470002000,"completed":1792470010000},"modelID":"mimo-v2-flash","providerID":"xiaomi","tokens":{"input":40,"output":4,"reasoning":0,"cache":{"read":0,"write":0}},"finish":"stop"}');
INSERT INTO message VALUES('msg_cc_1','ses_cc','main',1792400001000,'{"role":"assistant","time":{"created":1792400001000,"completed":1792400002000},"modelID":"claude-opus-4","providerID":"anthropic","cost":0,"tokens":{"input":9000,"output":900,"reasoning":0,"cache":{"read":0,"write":0}},"finish":"stop"}');
INSERT INTO message VALUES('msg_cc_2','ses_cc','main',1792470100000,'{"role":"assistant","time":{"created":1792470100000,"completed":1792470101000},"modelID":"mimo-v2-pro","providerID":"xiaomi","tokens":{"input":60,"output":6,"reasoning":0,"cache":{"read":0,"write":0}},"finish":"stop"}');
INSERT INTO message VALUES('msg_old','ses_old','main',1792300001000,'{"role":"assistant","time":{"created":1792300001000,"completed":1792300002000},"modelID":"claude-opus-4","providerID":"anthropic","tokens":{"input":7000,"output":700,"reasoning":0,"cache":{"read":0,"write":0}},"finish":"stop"}');
INSERT INTO external_import VALUES('cc','uuid-1','ses_cc','/Users/me/.claude/projects/app/uuid-1.jsonl',1792400000,1792400005000,'["msg_cc_1"]');
INSERT INTO external_import VALUES('cc','uuid-0','ses_old','/Users/me/.claude/projects/app/uuid-0.jsonl',1792300000,1792300005000,NULL);
