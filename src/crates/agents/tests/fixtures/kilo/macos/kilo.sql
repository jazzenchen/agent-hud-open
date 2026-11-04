-- A synthetic kilo.db, shaped as Kilo Code 7.8 writes it (Kilo-Org/kilocode
-- 080ca6c, packages/core/src/session/sql.ts): a session that ran two replies,
-- a child session, a task moved in from the VS Code extension with no tokens,
-- and a row whose data is not JSON.
CREATE TABLE session(
  id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, directory TEXT, title TEXT,
  time_created INTEGER, time_updated INTEGER, time_archived INTEGER,
  cost REAL, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER,
  tokens_cache_read INTEGER, tokens_cache_write INTEGER
);
CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
INSERT INTO session VALUES('ses_1','prj',NULL,'/Users/me/work/app','Fix the login form',1792470000000,1792470060000,NULL,0.2,1300,240,40,500,100);
INSERT INTO session VALUES('ses_2','prj','ses_1','/Users/me/work/app','Child session - 2026-10-20T10:00:00.000Z',1792470010000,1792470020000,NULL,0,0,0,0,0,0);
INSERT INTO session VALUES('ses_migrated_0123456789abcdef0123456789','prj',NULL,'/Users/me/work/old','Old task from the extension',1790000000000,1790000000000,NULL,0,0,0,0,0,0);
INSERT INTO message VALUES('msg_1','ses_1',1792470000000,1792470030000,'{"role":"user","time":{"created":1792470000000}}');
INSERT INTO message VALUES('msg_2','ses_1',1792470001000,1792470030000,'{"role":"assistant","time":{"created":1792470001000,"completed":1792470030000},"parentID":"msg_1","modelID":"claude-sonnet-4","providerID":"kilo","path":{"cwd":"/Users/me/work/app","root":"/Users/me/work/app"},"cost":0.12,"tokens":{"input":1000,"output":200,"reasoning":40,"cache":{"read":500,"write":100}},"finish":"stop"}');
INSERT INTO message VALUES('msg_3','ses_1',1792470040000,1792470060000,'{"role":"assistant","time":{"created":1792470040000,"completed":1792470060000},"parentID":"msg_1","modelID":"claude-sonnet-4","providerID":"kilo","tokens":{"input":300,"output":40,"reasoning":0,"cache":{"read":0,"write":0}},"finish":"tool-calls"}');
INSERT INTO message VALUES('msg_4','ses_2',1792470010000,1792470020000,'{"role":"assistant","time":{"created":1792470010000,"completed":1792470020000},"modelID":"gpt-5","providerID":"openai","tokens":{"input":50,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"finish":"stop"}');
INSERT INTO message VALUES('msg_migrated_0123456789abcdef0123456789','ses_migrated_0123456789abcdef0123456789',1790000000000,1790000000000,'{"role":"assistant","time":{"created":1790000000000,"completed":1790000000000},"modelID":"legacy","providerID":"legacy","cost":0,"tokens":{"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0}}}');
INSERT INTO message VALUES('msg_5','ses_1',1792470070000,1792470070000,'{"role":"assistant",');
