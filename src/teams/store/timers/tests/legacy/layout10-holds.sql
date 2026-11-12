PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE teams (
 id TEXT PRIMARY KEY, project TEXT NOT NULL, name TEXT NOT NULL, goal TEXT NOT NULL,
 home TEXT NOT NULL, owner TEXT NOT NULL, status TEXT NOT NULL, lead TEXT NOT NULL,
 settings TEXT NOT NULL, bundle TEXT NOT NULL, integration_branch TEXT NOT NULL,
 base_commit TEXT NOT NULL, revision INTEGER NOT NULL, person_paused INTEGER NOT NULL DEFAULT 0, next_node INTEGER NOT NULL DEFAULT 1
);
INSERT INTO teams VALUES('00000000-0000-4000-8000-00000000beef','example-project','Fixes','','00000000-0000-4000-8000-0000000000aa','local','"running"','lead','{"delegations":{"accept_results":true,"answer_members":true,"confirm_trust":true,"resume_exited":true,"merge_accepted":true},"caps":{"max_members":6,"max_phase_tasks":50,"max_open_tasks":500,"max_open_questions":3},"timers":{"report_timeout_seconds":120,"start_timeout_seconds":180,"inbox_interval_seconds":120,"remind_lead_minutes":15,"remind_person_minutes":60,"lead_stall_minutes":20,"answer_escalate_minutes":10,"resume_limit_per_hour":3},"queue":true,"routing":"direct","plan_approval":"review","phase_acceptance":"lead","integration":"branch","land_allowed":false}','{"template":{"id":"bugfix","name":"Bugfix","description":"A queue of independent fixes: a developer and a reviewer.","lead":{"agent":"claude"},"roster":[{"role":"developer","count":2},{"role":"reviewer","count":1}],"default_pipeline":"reviewed","settings":{"delegations":{"accept_results":true,"answer_members":true,"confirm_trust":true,"resume_exited":true,"merge_accepted":true},"caps":{"max_members":6,"max_phase_tasks":50,"max_open_tasks":500,"max_open_questions":3},"timers":{"report_timeout_seconds":120,"start_timeout_seconds":180,"inbox_interval_seconds":120,"remind_lead_minutes":15,"remind_person_minutes":60,"lead_stall_minutes":20,"answer_escalate_minutes":10,"resume_limit_per_hour":3},"queue":true,"routing":"direct","plan_approval":"review","phase_acceptance":"lead","integration":"branch","land_allowed":false}},"roles":[{"id":"developer","name":"Developer","prompt":"Implement the task in your own worktree. Start from the base commit named in the task, keep to its write set, run the checks that can fail for your change, commit, and report what you changed and how you checked it.","agent":"claude"},{"id":"reviewer","name":"Reviewer","prompt":"Review the reported commit against the task''s criteria. You never change what you review. Give a verdict: pass, or return with the exact findings.","agent":"claude"}],"pipelines":[{"id":"reviewed","name":"Reviewed: review, lead","implementer":"developer"}]}','zerus/team/fixes','0123456789abcdef0123456789abcdef01234567',64,0,8);
CREATE TABLE members (
 team TEXT NOT NULL REFERENCES teams(id), id TEXT NOT NULL, data TEXT NOT NULL,
 machine TEXT, session_id TEXT, state TEXT NOT NULL, person_stopped INTEGER NOT NULL DEFAULT 0,
 PRIMARY KEY(team,id), UNIQUE(machine,session_id)
);
INSERT INTO members VALUES('00000000-0000-4000-8000-00000000beef','lead','{"id":"lead","role":"lead","agent":"claude","account":"account-example","state":"active","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"},"worktree":"/workspace/example-team-lead","ownership_nonce":"nonce-lead"}','00000000-0000-4000-8000-0000000000aa','launch_id:00000000-0000-4000-8000-000000000064','"active"',0);
INSERT INTO members VALUES('00000000-0000-4000-8000-00000000beef','m1','{"id":"m1","role":"developer","agent":"claude","account":"account-example","state":"active","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000031"},"worktree":"/workspace/example-team-m1","ownership_nonce":"nonce-m1"}','00000000-0000-4000-8000-0000000000aa','launch_id:00000000-0000-4000-8000-000000000031','"active"',0);
INSERT INTO members VALUES('00000000-0000-4000-8000-00000000beef','m2','{"id":"m2","role":"developer","agent":"claude","account":"account-example","state":"active","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000032"},"worktree":"/workspace/example-team-m2","ownership_nonce":"nonce-m2"}','00000000-0000-4000-8000-0000000000aa','launch_id:00000000-0000-4000-8000-000000000032','"active"',0);
INSERT INTO members VALUES('00000000-0000-4000-8000-00000000beef','m3','{"id":"m3","role":"reviewer","agent":"claude"}',NULL,NULL,'"not_created"',0);
CREATE TABLE phases (team TEXT NOT NULL REFERENCES teams(id), n INTEGER NOT NULL,
 data TEXT NOT NULL, PRIMARY KEY(team,n));
INSERT INTO phases VALUES('00000000-0000-4000-8000-00000000beef',0,'{"n":0,"title":"Queue","kind":"queue","mode":"free","state":"approved"}');
CREATE TABLE node_identity (team TEXT NOT NULL REFERENCES teams(id), id INTEGER NOT NULL,
 task_project TEXT NOT NULL, task_uuid TEXT NOT NULL, PRIMARY KEY(team,id));
INSERT INTO node_identity VALUES('00000000-0000-4000-8000-00000000beef',1,'example-project','00000000-0000-4000-8000-000000000001');
INSERT INTO node_identity VALUES('00000000-0000-4000-8000-00000000beef',2,'example-project','00000000-0000-4000-8000-000000000002');
INSERT INTO node_identity VALUES('00000000-0000-4000-8000-00000000beef',3,'example-project','00000000-0000-4000-8000-000000000003');
INSERT INTO node_identity VALUES('00000000-0000-4000-8000-00000000beef',4,'example-project','00000000-0000-4000-8000-000000000004');
INSERT INTO node_identity VALUES('00000000-0000-4000-8000-00000000beef',5,'example-project','00000000-0000-4000-8000-000000000005');
INSERT INTO node_identity VALUES('00000000-0000-4000-8000-00000000beef',6,'example-project','00000000-0000-4000-8000-000000000006');
INSERT INTO node_identity VALUES('00000000-0000-4000-8000-00000000beef',7,'example-project','00000000-0000-4000-8000-000000000007');
CREATE TABLE live_tasks (task_project TEXT NOT NULL, task_uuid TEXT NOT NULL, team TEXT NOT NULL, node INTEGER NOT NULL,
 PRIMARY KEY(task_project,task_uuid), FOREIGN KEY(team,node) REFERENCES node_identity(team,id));
INSERT INTO live_tasks VALUES('example-project','00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-00000000beef',1);
INSERT INTO live_tasks VALUES('example-project','00000000-0000-4000-8000-000000000002','00000000-0000-4000-8000-00000000beef',2);
INSERT INTO live_tasks VALUES('example-project','00000000-0000-4000-8000-000000000003','00000000-0000-4000-8000-00000000beef',3);
INSERT INTO live_tasks VALUES('example-project','00000000-0000-4000-8000-000000000004','00000000-0000-4000-8000-00000000beef',4);
INSERT INTO live_tasks VALUES('example-project','00000000-0000-4000-8000-000000000005','00000000-0000-4000-8000-00000000beef',5);
INSERT INTO live_tasks VALUES('example-project','00000000-0000-4000-8000-000000000006','00000000-0000-4000-8000-00000000beef',6);
INSERT INTO live_tasks VALUES('example-project','00000000-0000-4000-8000-000000000007','00000000-0000-4000-8000-00000000beef',7);
CREATE TABLE suggestions (team TEXT NOT NULL, node INTEGER NOT NULL, role TEXT, pipeline TEXT, PRIMARY KEY(team,node),
 FOREIGN KEY(team,node) REFERENCES node_identity(team,id));
CREATE TABLE nodes (
 team TEXT NOT NULL, id INTEGER NOT NULL, data TEXT NOT NULL, rank TEXT NOT NULL,
 admitted INTEGER NOT NULL, routed INTEGER NOT NULL, hold INTEGER NOT NULL DEFAULT 0,
 task_revision TEXT NOT NULL, task_stage TEXT NOT NULL, task_cancelled INTEGER NOT NULL,
 generation INTEGER NOT NULL DEFAULT 1, decided_by TEXT, integrated INTEGER NOT NULL DEFAULT 0,
 integration_commit TEXT, requested TEXT, integration_required INTEGER NOT NULL DEFAULT 1,
 node_state TEXT GENERATED ALWAYS AS (CASE
   WHEN task_cancelled THEN 'cancelled'
   WHEN task_stage='done' AND integration_required AND COALESCE(json_extract(data,'$.kind'),'task')<>'stage' AND NOT integrated THEN 'reported'
   WHEN task_stage='done' THEN 'accepted'
   WHEN task_stage='unobserved' THEN 'waiting'
   WHEN NOT admitted OR NOT routed OR task_stage='backlog' THEN 'waiting'
   WHEN task_stage='in_progress' THEN 'in_progress'
   WHEN task_stage='in_review' THEN 'reported' ELSE 'pending' END) VIRTUAL,
 PRIMARY KEY(team,id), FOREIGN KEY(team,id) REFERENCES node_identity(team,id)
);
INSERT INTO nodes VALUES('00000000-0000-4000-8000-00000000beef',1,'{"id":1,"implementer":"m1","phase":0,"pipeline":"reviewed","role":"developer","round":1,"source":"sent","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000001"},"title":"Task 1"}','a00000001',1,1,1,'token-3','in_review',0,1,NULL,0,NULL,NULL,1);
INSERT INTO nodes VALUES('00000000-0000-4000-8000-00000000beef',2,'{"id":2,"implementer":"m1","phase":0,"pipeline":"reviewed","role":"developer","round":1,"source":"sent","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000002"},"title":"Task 2"}','a00000002',1,1,1,'token-6','in_review',0,1,NULL,0,NULL,NULL,1);
INSERT INTO nodes VALUES('00000000-0000-4000-8000-00000000beef',3,'{"id":3,"phase":0,"pipeline":"reviewed","role":"developer","round":1,"source":"sent","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000003"},"title":"Task 3"}','a00000003',1,1,1,'token-7','todo',0,1,NULL,0,NULL,NULL,1);
INSERT INTO nodes VALUES('00000000-0000-4000-8000-00000000beef',4,'{"id":4,"implementer":"m1","phase":0,"pipeline":"reviewed","role":"developer","round":1,"source":"sent","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000004"},"title":"Task 4"}','a00000004',1,1,1,'token-10','in_review',0,1,NULL,0,NULL,NULL,1);
INSERT INTO nodes VALUES('00000000-0000-4000-8000-00000000beef',5,'{"id":5,"phase":0,"pipeline":"reviewed","role":"developer","round":2,"source":"sent","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000005"},"title":"Task 5"}','a00000005',1,1,1,'token-13','in_review',0,2,NULL,0,NULL,NULL,1);
INSERT INTO nodes VALUES('00000000-0000-4000-8000-00000000beef',6,'{"id":6,"phase":0,"pipeline":"reviewed","role":"developer","round":2,"source":"sent","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000006"},"title":"Task 6"}','a00000006',1,1,1,'token-16','in_review',0,2,NULL,0,NULL,NULL,1);
INSERT INTO nodes VALUES('00000000-0000-4000-8000-00000000beef',7,'{"id":7,"phase":0,"pipeline":"reviewed","role":"developer","round":1,"source":"sent","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000007"},"title":"Task 7"}','a00000007',1,1,1,'token-17','todo',0,1,NULL,0,NULL,NULL,1);
CREATE TABLE node_archive (
 team TEXT NOT NULL, id INTEGER NOT NULL, data TEXT NOT NULL, rank TEXT NOT NULL,
 admitted INTEGER NOT NULL, routed INTEGER NOT NULL, hold INTEGER NOT NULL DEFAULT 0,
 task_revision TEXT NOT NULL, task_stage TEXT NOT NULL, task_cancelled INTEGER NOT NULL,
 generation INTEGER NOT NULL DEFAULT 1, decided_by TEXT, integrated INTEGER NOT NULL DEFAULT 0,
 integration_commit TEXT, requested TEXT, integration_required INTEGER NOT NULL DEFAULT 1,
 node_state TEXT GENERATED ALWAYS AS (CASE
   WHEN task_cancelled THEN 'cancelled'
   WHEN task_stage='done' AND integration_required AND COALESCE(json_extract(data,'$.kind'),'task')<>'stage' AND NOT integrated THEN 'reported'
   WHEN task_stage='done' THEN 'accepted'
   WHEN task_stage='unobserved' THEN 'waiting'
   WHEN NOT admitted OR NOT routed OR task_stage='backlog' THEN 'waiting'
   WHEN task_stage='in_progress' THEN 'in_progress'
   WHEN task_stage='in_review' THEN 'reported' ELSE 'pending' END) VIRTUAL,
 PRIMARY KEY(team,id), FOREIGN KEY(team,id) REFERENCES node_identity(team,id)
);
CREATE TABLE dependencies (team TEXT NOT NULL, node INTEGER NOT NULL, dependency INTEGER NOT NULL,
 kind TEXT NOT NULL, PRIMARY KEY(team,node,dependency),
 FOREIGN KEY(team,node) REFERENCES node_identity(team,id),
 FOREIGN KEY(team,dependency) REFERENCES node_identity(team,id));
CREATE TABLE inbox (seq INTEGER PRIMARY KEY AUTOINCREMENT, team TEXT NOT NULL REFERENCES teams(id),
 kind TEXT NOT NULL, node INTEGER NOT NULL, generation INTEGER NOT NULL, actor TEXT NOT NULL,
 text TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'open', reply TEXT, reply_actor TEXT,
 created_ms INTEGER NOT NULL, UNIQUE(team,kind,node,generation));
INSERT INTO inbox VALUES(1,'00000000-0000-4000-8000-00000000beef','report',1,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(2,'00000000-0000-4000-8000-00000000beef','cancel_refused:1',1,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','The cancel of task 1 was refused: Still running','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(3,'00000000-0000-4000-8000-00000000beef','report',2,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(4,'00000000-0000-4000-8000-00000000beef','cancel_refused:2',2,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','The cancel of task 2 was refused: Still running','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(5,'00000000-0000-4000-8000-00000000beef','report',4,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(6,'00000000-0000-4000-8000-00000000beef','cancel_refused:3',4,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','The cancel of task 4 was refused: Still running','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(7,'00000000-0000-4000-8000-00000000beef','report',5,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(8,'00000000-0000-4000-8000-00000000beef','report',6,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','open',NULL,NULL,1234);
INSERT INTO inbox VALUES(9,'00000000-0000-4000-8000-00000000beef','delivery_uncertain',6,2,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Replacement starting','open',NULL,NULL,1234);
CREATE TABLE cards (seq INTEGER PRIMARY KEY AUTOINCREMENT, team TEXT NOT NULL REFERENCES teams(id),
 kind TEXT NOT NULL, node INTEGER, actor TEXT NOT NULL, text TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'open', decision TEXT, decided_actor TEXT, created_ms INTEGER NOT NULL, decision_seq INTEGER, decisions TEXT);
CREATE TABLE journal (seq INTEGER PRIMARY KEY AUTOINCREMENT, team TEXT NOT NULL REFERENCES teams(id),
 at_ms INTEGER NOT NULL, actor TEXT NOT NULL, kind TEXT NOT NULL, request_id TEXT NOT NULL, text TEXT NOT NULL,
 refs TEXT NOT NULL DEFAULT '[]');
INSERT INTO journal VALUES(1,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','create','f8e026b6-12c7-4a35-b711-5d759118e33f','create at revision 1','["team:00000000-0000-4000-8000-00000000beef"]');
INSERT INTO journal VALUES(2,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','set_member_session','ebf44aca-1510-4af7-a13f-aafd6a74dde4','set_member_session at revision 2','["team:00000000-0000-4000-8000-00000000beef","member:lead"]');
INSERT INTO journal VALUES(3,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','set_member_session','0320d443-1ca7-4aae-98e7-f5fdd71f3bfa','set_member_session at revision 3','["team:00000000-0000-4000-8000-00000000beef","member:m1"]');
INSERT INTO journal VALUES(4,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','set_member_session','3e2c3f23-3b24-4ba2-bb75-adbdc322a97c','set_member_session at revision 4','["team:00000000-0000-4000-8000-00000000beef","member:m2"]');
INSERT INTO journal VALUES(5,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','2636ee08-848e-430d-b68a-f9fc63d9b519','add_task at revision 5','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000001"]');
INSERT INTO journal VALUES(6,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','29ec82eb-72bc-4a59-b8d5-6971681dd355','observe_task at revision 6','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(7,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','2f715bc1-eaeb-4e95-82bf-8aef059c6596','handout_intent at revision 7','["team:00000000-0000-4000-8000-00000000beef","node:1","member:m1","effect:224ecd68-4b3e-49b1-8336-019148e68502"]');
INSERT INTO journal VALUES(8,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','5c008678-4d12-43a0-84de-26001297a0a1','handout_outcome at revision 8','["team:00000000-0000-4000-8000-00000000beef","effect:224ecd68-4b3e-49b1-8336-019148e68502"]');
INSERT INTO journal VALUES(9,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','be3db260-761c-42f4-9ebd-6b2e1204a909','observe_task at revision 9','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(10,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','e88ef418-cb5b-4739-95c3-cb48d41e4860','observe_task at revision 10','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(11,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','6cd78e5e-e7fc-414c-a816-3b0cb3c64d78','record_report at revision 11','["team:00000000-0000-4000-8000-00000000beef","node:1","member:m1"]');
INSERT INTO journal VALUES(12,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','0baaca7e-5eb8-4cf3-9060-b2712a1891af','cancel at revision 12','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(13,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_cancel','a62703f4-a9ce-4bb9-9095-8f259f17355f','record_cancel at revision 13','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(14,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','release','15d9f872-240a-4c59-89c5-c9a457ea1e4a','release at revision 14','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(15,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','2ed008be-3ef8-40f6-a023-dcb5432bdeb5','hold at revision 15','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(16,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','11f3f3ef-964b-4cf0-b88a-bbf5f33fddda','add_task at revision 16','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000002"]');
INSERT INTO journal VALUES(17,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','7c6eb4b0-ec10-4865-967e-4dc8264ef01c','observe_task at revision 17','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(18,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','a3bd0eb3-2a60-436c-9dd8-3bbca42ed4e4','handout_intent at revision 18','["team:00000000-0000-4000-8000-00000000beef","node:2","member:m1","effect:d2456c7d-e25e-4fe5-a6a0-3a2b247f057c"]');
INSERT INTO journal VALUES(19,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','bb93cfae-0c66-498a-8a53-7223eaa4fe90','handout_outcome at revision 19','["team:00000000-0000-4000-8000-00000000beef","effect:d2456c7d-e25e-4fe5-a6a0-3a2b247f057c"]');
INSERT INTO journal VALUES(20,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','9b62c3dc-e451-49b2-9a0d-2c11663e65ae','observe_task at revision 20','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(21,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','6614883d-c873-4ea1-910f-f12749510503','observe_task at revision 21','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(22,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','04f181cd-b10d-4d16-8af3-6da7847628f2','record_report at revision 22','["team:00000000-0000-4000-8000-00000000beef","node:2","member:m1"]');
INSERT INTO journal VALUES(23,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','c02083f7-3af0-4e37-9301-bf74703699a8','hold at revision 23','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(24,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','4e9a950c-75a6-4ba6-8aee-a11724319a03','cancel at revision 24','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(25,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_cancel','e6ce526f-a641-441e-9f20-23f894ab2867','record_cancel at revision 25','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(26,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','3ed52fe2-c6ef-4c0c-b9f9-408b0ee3ba70','add_task at revision 26','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000003"]');
INSERT INTO journal VALUES(27,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','60df581b-adb8-4d2e-8627-26868f0d5302','observe_task at revision 27','["team:00000000-0000-4000-8000-00000000beef","node:3"]');
INSERT INTO journal VALUES(28,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','hold','e3fef2c9-911c-4745-bee5-63be9347c34f','hold at revision 28','["team:00000000-0000-4000-8000-00000000beef","node:3"]');
INSERT INTO journal VALUES(29,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','aad51046-3e24-4b27-8f75-67c9bfe08059','add_task at revision 29','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000004"]');
INSERT INTO journal VALUES(30,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','646a3012-4d7d-466f-b981-482725a1085b','observe_task at revision 30','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(31,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','5bc8b0b4-0cf5-4e3a-9049-59272f8310c7','handout_intent at revision 31','["team:00000000-0000-4000-8000-00000000beef","node:4","member:m1","effect:a0ad0e3b-94bf-451e-bec5-ced2afa472df"]');
INSERT INTO journal VALUES(32,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','3d38072d-f7d0-4adc-a56a-2c49630dde5b','handout_outcome at revision 32','["team:00000000-0000-4000-8000-00000000beef","effect:a0ad0e3b-94bf-451e-bec5-ced2afa472df"]');
INSERT INTO journal VALUES(33,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','5a31439b-59dd-4597-84ea-ccabb4083881','observe_task at revision 33','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(34,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','81092f08-784e-43ae-a3a4-41e6529d0203','observe_task at revision 34','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(35,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','d40ac0d6-f479-423b-8ae2-c41a95279e92','record_report at revision 35','["team:00000000-0000-4000-8000-00000000beef","node:4","member:m1"]');
INSERT INTO journal VALUES(36,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','85a79d23-8d75-4893-8438-3e8f84f4db4f','cancel at revision 36','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(37,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_cancel','b3423f5a-4a75-4585-8e63-c4f268782bcf','record_cancel at revision 37','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(38,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','release','9b5981d2-12c7-4f5b-9477-a34d83eb93d5','release at revision 38','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(39,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','hold','1607f6d1-fd4f-40e0-9b0c-3313b035d78e','hold at revision 39','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(40,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','159fc2a4-6844-42da-a08a-6349356d8184','add_task at revision 40','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000005"]');
INSERT INTO journal VALUES(41,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','9fd74a6b-928a-40d2-b4fc-bcc16631772c','observe_task at revision 41','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(42,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','1c75f63d-ab4b-492b-9c30-ff1eeccc509e','handout_intent at revision 42','["team:00000000-0000-4000-8000-00000000beef","node:5","member:m1","effect:ceb5ecbb-207a-4837-91ad-86ff905163fa"]');
INSERT INTO journal VALUES(43,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','3a54bb77-43b7-40f8-bd8f-35e41fc3af43','handout_outcome at revision 43','["team:00000000-0000-4000-8000-00000000beef","effect:ceb5ecbb-207a-4837-91ad-86ff905163fa"]');
INSERT INTO journal VALUES(44,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','dcd185cf-085b-404f-84b5-05181c866c6d','observe_task at revision 44','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(45,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','7f10c246-9166-4127-94bf-6ba645d5130d','observe_task at revision 45','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(46,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','6daafa1c-1596-4b84-8972-7574c89ed345','record_report at revision 46','["team:00000000-0000-4000-8000-00000000beef","node:5","member:m1"]');
INSERT INTO journal VALUES(47,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','return','3209e7a5-5911-4e98-84f8-0eb9c4a64061','return at revision 47','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(48,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_intent','137e5f4c-adce-4eb1-bfff-2cbbad98ae94','reopen_intent at revision 48','["team:00000000-0000-4000-8000-00000000beef","node:5","member:m1","effect:85f6ee47-fec1-4ce3-9cf8-9b0421bffd42"]');
INSERT INTO journal VALUES(49,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_outcome','50363d9b-d98c-4e55-96f7-2368cc9b425e','reopen_outcome at revision 49','["team:00000000-0000-4000-8000-00000000beef","effect:85f6ee47-fec1-4ce3-9cf8-9b0421bffd42"]');
INSERT INTO journal VALUES(50,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','dc14676e-3b74-423c-b478-f7d58dbf657a','add_task at revision 50','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000006"]');
INSERT INTO journal VALUES(51,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','df392e11-c8c5-499a-9f80-66090f304e72','observe_task at revision 51','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(52,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','59af276b-4ca3-4fc1-88b1-475c6714b1ea','handout_intent at revision 52','["team:00000000-0000-4000-8000-00000000beef","node:6","member:m1","effect:9fd55718-fd04-4655-8b80-e1aaeddf6fd2"]');
INSERT INTO journal VALUES(53,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','4865a1a3-25ca-4230-9cf0-74223865eb74','handout_outcome at revision 53','["team:00000000-0000-4000-8000-00000000beef","effect:9fd55718-fd04-4655-8b80-e1aaeddf6fd2"]');
INSERT INTO journal VALUES(54,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','abf260b3-5438-46f8-8a4e-f1de6a7871da','observe_task at revision 54','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(55,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','78f50112-815a-4a3d-8324-118075a126ad','observe_task at revision 55','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(56,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','29bc35e6-d622-43c5-ac4d-11d170f32ceb','record_report at revision 56','["team:00000000-0000-4000-8000-00000000beef","node:6","member:m1"]');
INSERT INTO journal VALUES(57,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','return','7d658e93-3fe6-477c-b19b-5770cdee6b82','return at revision 57','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(58,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_intent','5a9ed9ff-cc38-4d48-8c27-c70dccb31e9b','reopen_intent at revision 58','["team:00000000-0000-4000-8000-00000000beef","node:6","member:m1","effect:4730d997-9033-4f38-9b72-6435ac8c0f96"]');
INSERT INTO journal VALUES(59,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_outcome','bfa53685-4ef4-45b0-a47e-2509eeb1b6ba','reopen_outcome at revision 59','["team:00000000-0000-4000-8000-00000000beef","effect:4730d997-9033-4f38-9b72-6435ac8c0f96"]');
INSERT INTO journal VALUES(60,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','release','b1f9be4e-591e-4006-a263-ecb64d227d33','release at revision 60','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(61,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','ae9142bc-1897-4209-b183-be63303c7714','hold at revision 61','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(62,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','3b602bbf-0e81-440b-bd34-2130a42d4606','add_task at revision 62','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000007"]');
INSERT INTO journal VALUES(63,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','67478ad2-e297-41a5-945c-d92d4c0698e9','observe_task at revision 63','["team:00000000-0000-4000-8000-00000000beef","node:7"]');
INSERT INTO journal VALUES(64,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','af1e2a8e-8a00-490a-be2c-e0bce2d7c23f','hold at revision 64','["team:00000000-0000-4000-8000-00000000beef","node:7"]');
CREATE TABLE journal_archive(
  seq INT,
  team TEXT,
  at_ms INT,
  actor TEXT,
  kind TEXT,
  request_id TEXT,
  text TEXT,
  refs TEXT
);
CREATE TABLE effects (seq INTEGER PRIMARY KEY AUTOINCREMENT, team TEXT NOT NULL, node INTEGER NOT NULL,
 member TEXT NOT NULL, generation INTEGER NOT NULL, request_id TEXT NOT NULL UNIQUE,
 actor TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'handout', state TEXT NOT NULL DEFAULT 'reserved', outcome TEXT, created_ms INTEGER NOT NULL,
 FOREIGN KEY(team,node) REFERENCES node_identity(team,id), FOREIGN KEY(team,member) REFERENCES members(team,id));
INSERT INTO effects VALUES(1,'00000000-0000-4000-8000-00000000beef',1,'m1',1,'224ecd68-4b3e-49b1-8336-019148e68502','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(2,'00000000-0000-4000-8000-00000000beef',2,'m1',1,'d2456c7d-e25e-4fe5-a6a0-3a2b247f057c','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(3,'00000000-0000-4000-8000-00000000beef',4,'m1',1,'a0ad0e3b-94bf-451e-bec5-ced2afa472df','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(4,'00000000-0000-4000-8000-00000000beef',5,'m1',1,'ceb5ecbb-207a-4837-91ad-86ff905163fa','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(5,'00000000-0000-4000-8000-00000000beef',5,'m1',2,'85f6ee47-fec1-4ce3-9cf8-9b0421bffd42','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen','refused','{"status":"refused","reason":"Not delivered"}',1234);
INSERT INTO effects VALUES(6,'00000000-0000-4000-8000-00000000beef',6,'m1',1,'9fd55718-fd04-4655-8b80-e1aaeddf6fd2','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(7,'00000000-0000-4000-8000-00000000beef',6,'m1',2,'4730d997-9033-4f38-9b72-6435ac8c0f96','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen','cutting_over','{"status":"cutting_over","reason":"Replacement starting"}',1234);
CREATE TABLE reports (team TEXT NOT NULL, node INTEGER NOT NULL, generation INTEGER NOT NULL,
 actor TEXT NOT NULL, text TEXT NOT NULL, commit_id TEXT NOT NULL, PRIMARY KEY(team,node,generation),
 FOREIGN KEY(team,node) REFERENCES node_identity(team,id));
INSERT INTO reports VALUES('00000000-0000-4000-8000-00000000beef',1,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','0123456789abcdef0123456789abcdef01234567');
INSERT INTO reports VALUES('00000000-0000-4000-8000-00000000beef',2,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','0123456789abcdef0123456789abcdef01234567');
INSERT INTO reports VALUES('00000000-0000-4000-8000-00000000beef',4,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','0123456789abcdef0123456789abcdef01234567');
INSERT INTO reports VALUES('00000000-0000-4000-8000-00000000beef',5,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','0123456789abcdef0123456789abcdef01234567');
INSERT INTO reports VALUES('00000000-0000-4000-8000-00000000beef',6,1,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','Implemented and checked','0123456789abcdef0123456789abcdef01234567');
CREATE TABLE decisions (seq INTEGER PRIMARY KEY AUTOINCREMENT, team TEXT NOT NULL, node INTEGER NOT NULL,
 generation INTEGER NOT NULL, actor TEXT NOT NULL, kind TEXT NOT NULL, text TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending', stopped_ready INTEGER NOT NULL DEFAULT 0,
 FOREIGN KEY(team,node) REFERENCES node_identity(team,id));
INSERT INTO decisions VALUES(1,'00000000-0000-4000-8000-00000000beef',1,1,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','Cancel for the legacy hold','settled',0);
INSERT INTO decisions VALUES(2,'00000000-0000-4000-8000-00000000beef',2,1,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','Cancel for the legacy hold','settled',0);
INSERT INTO decisions VALUES(3,'00000000-0000-4000-8000-00000000beef',4,1,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','Cancel for the legacy hold','settled',0);
INSERT INTO decisions VALUES(4,'00000000-0000-4000-8000-00000000beef',5,2,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','return','Reopen for the legacy hold','settled',0);
INSERT INTO decisions VALUES(5,'00000000-0000-4000-8000-00000000beef',6,2,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','return','Reopen for the legacy hold','settled',0);
CREATE TABLE integration_facts (decision INTEGER PRIMARY KEY REFERENCES decisions(seq),
 team TEXT NOT NULL, node INTEGER NOT NULL, actor TEXT NOT NULL,
 commit_id TEXT, conflicts TEXT NOT NULL, created_ms INTEGER NOT NULL,
 FOREIGN KEY(team,node) REFERENCES node_identity(team,id));
CREATE TABLE requests (id TEXT PRIMARY KEY, team TEXT NOT NULL, actor TEXT NOT NULL,
 content TEXT NOT NULL, result TEXT NOT NULL);
INSERT INTO requests VALUES('f8e026b6-12c7-4a35-b711-5d759118e33f','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"f8e026b6-12c7-4a35-b711-5d759118e33f","team":"00000000-0000-4000-8000-00000000beef","expected_revision":0,"op":"create","project":"example-project","name":"Fixes","goal":"","home":"00000000-0000-4000-8000-0000000000aa","bundle":{"template":{"id":"bugfix","name":"Bugfix","description":"A queue of independent fixes: a developer and a reviewer.","lead":{"agent":"claude"},"roster":[{"role":"developer","count":2},{"role":"reviewer","count":1}],"default_pipeline":"reviewed","settings":{"delegations":{"accept_results":true,"answer_members":true,"confirm_trust":true,"resume_exited":true,"merge_accepted":true},"caps":{"max_members":6,"max_phase_tasks":50,"max_open_tasks":500,"max_open_questions":3},"timers":{"report_timeout_seconds":120,"start_timeout_seconds":180,"inbox_interval_seconds":120,"remind_lead_minutes":15,"remind_person_minutes":60,"lead_stall_minutes":20,"answer_escalate_minutes":10,"resume_limit_per_hour":3},"queue":true,"routing":"direct","plan_approval":"review","phase_acceptance":"lead","integration":"branch","land_allowed":false}},"roles":[{"id":"developer","name":"Developer","prompt":"Implement the task in your own worktree. Start from the base commit named in the task, keep to its write set, run the checks that can fail for your change, commit, and report what you changed and how you checked it.","agent":"claude"},{"id":"reviewer","name":"Reviewer","prompt":"Review the reported commit against the task''s criteria. You never change what you review. Give a verdict: pass, or return with the exact findings.","agent":"claude"}],"pipelines":[{"id":"reviewed","name":"Reviewed: review, lead","implementer":"developer"}]},"integration_branch":"zerus/team/fixes","base_commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":1}}');
INSERT INTO requests VALUES('ebf44aca-1510-4af7-a13f-aafd6a74dde4','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"ebf44aca-1510-4af7-a13f-aafd6a74dde4","team":"00000000-0000-4000-8000-00000000beef","expected_revision":1,"op":"set_member_session","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"},"account":"account-example","worktree":"/workspace/example-team-lead","ownership_nonce":"nonce-lead"}','{"Success":{"ok":true,"revision":2}}');
INSERT INTO requests VALUES('0320d443-1ca7-4aae-98e7-f5fdd71f3bfa','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"0320d443-1ca7-4aae-98e7-f5fdd71f3bfa","team":"00000000-0000-4000-8000-00000000beef","expected_revision":2,"op":"set_member_session","member":"m1","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000031"},"account":"account-example","worktree":"/workspace/example-team-m1","ownership_nonce":"nonce-m1"}','{"Success":{"ok":true,"revision":3}}');
INSERT INTO requests VALUES('3e2c3f23-3b24-4ba2-bb75-adbdc322a97c','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"3e2c3f23-3b24-4ba2-bb75-adbdc322a97c","team":"00000000-0000-4000-8000-00000000beef","expected_revision":3,"op":"set_member_session","member":"m2","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000032"},"account":"account-example","worktree":"/workspace/example-team-m2","ownership_nonce":"nonce-m2"}','{"Success":{"ok":true,"revision":4}}');
INSERT INTO requests VALUES('2636ee08-848e-430d-b68a-f9fc63d9b519','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"2636ee08-848e-430d-b68a-f9fc63d9b519","team":"00000000-0000-4000-8000-00000000beef","expected_revision":4,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000001"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":5,"node":1}}');
INSERT INTO requests VALUES('29ec82eb-72bc-4a59-b8d5-6971681dd355','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"29ec82eb-72bc-4a59-b8d5-6971681dd355","team":"00000000-0000-4000-8000-00000000beef","expected_revision":5,"op":"observe_task","node":1,"observation":{"revision":"token-1","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000001","title":"Task 1","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":6}}');
INSERT INTO requests VALUES('2f715bc1-eaeb-4e95-82bf-8aef059c6596','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"2f715bc1-eaeb-4e95-82bf-8aef059c6596","team":"00000000-0000-4000-8000-00000000beef","expected_revision":6,"op":"handout_intent","node":1,"member":"m1","effect_request_id":"224ecd68-4b3e-49b1-8336-019148e68502"}','{"Success":{"ok":true,"revision":7,"node":1,"item":1}}');
INSERT INTO requests VALUES('5c008678-4d12-43a0-84de-26001297a0a1','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"5c008678-4d12-43a0-84de-26001297a0a1","team":"00000000-0000-4000-8000-00000000beef","expected_revision":7,"op":"handout_outcome","effect_request_id":"224ecd68-4b3e-49b1-8336-019148e68502","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":8}}');
INSERT INTO requests VALUES('be3db260-761c-42f4-9ebd-6b2e1204a909','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"be3db260-761c-42f4-9ebd-6b2e1204a909","team":"00000000-0000-4000-8000-00000000beef","expected_revision":8,"op":"observe_task","node":1,"observation":{"revision":"token-2","previous_revision":"token-1","stage":"in_progress","cancelled":false,"rank":"a00000001","title":"Task 1","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":9}}');
INSERT INTO requests VALUES('e88ef418-cb5b-4739-95c3-cb48d41e4860','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"e88ef418-cb5b-4739-95c3-cb48d41e4860","team":"00000000-0000-4000-8000-00000000beef","expected_revision":9,"op":"observe_task","node":1,"observation":{"revision":"token-3","previous_revision":"token-2","stage":"in_review","cancelled":false,"rank":"a00000001","title":"Task 1","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":10}}');
INSERT INTO requests VALUES('6cd78e5e-e7fc-414c-a816-3b0cb3c64d78','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"6cd78e5e-e7fc-414c-a816-3b0cb3c64d78","team":"00000000-0000-4000-8000-00000000beef","expected_revision":10,"op":"record_report","node":1,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":11,"item":1}}');
INSERT INTO requests VALUES('0baaca7e-5eb8-4cf3-9060-b2712a1891af','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"0baaca7e-5eb8-4cf3-9060-b2712a1891af","team":"00000000-0000-4000-8000-00000000beef","expected_revision":11,"op":"cancel","node":1,"reason":"Cancel for the legacy hold"}','{"Success":{"ok":true,"revision":12}}');
INSERT INTO requests VALUES('a62703f4-a9ce-4bb9-9095-8f259f17355f','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"a62703f4-a9ce-4bb9-9095-8f259f17355f","team":"00000000-0000-4000-8000-00000000beef","expected_revision":12,"op":"record_cancel","node":1,"decision":1,"state":"refused","reason":"Still running","attempt":0,"attempt_id":""}','{"Success":{"ok":true,"revision":13,"node":1,"item":2}}');
INSERT INTO requests VALUES('15d9f872-240a-4c59-89c5-c9a457ea1e4a','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"15d9f872-240a-4c59-89c5-c9a457ea1e4a","team":"00000000-0000-4000-8000-00000000beef","expected_revision":13,"op":"release","node":1}','{"Success":{"ok":true,"revision":14}}');
INSERT INTO requests VALUES('2ed008be-3ef8-40f6-a023-dcb5432bdeb5','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"2ed008be-3ef8-40f6-a023-dcb5432bdeb5","team":"00000000-0000-4000-8000-00000000beef","expected_revision":14,"op":"hold","node":1}','{"Success":{"ok":true,"revision":15}}');
INSERT INTO requests VALUES('11f3f3ef-964b-4cf0-b88a-bbf5f33fddda','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"11f3f3ef-964b-4cf0-b88a-bbf5f33fddda","team":"00000000-0000-4000-8000-00000000beef","expected_revision":15,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000002"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":16,"node":2}}');
INSERT INTO requests VALUES('7c6eb4b0-ec10-4865-967e-4dc8264ef01c','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"7c6eb4b0-ec10-4865-967e-4dc8264ef01c","team":"00000000-0000-4000-8000-00000000beef","expected_revision":16,"op":"observe_task","node":2,"observation":{"revision":"token-4","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000002","title":"Task 2","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":17}}');
INSERT INTO requests VALUES('a3bd0eb3-2a60-436c-9dd8-3bbca42ed4e4','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"a3bd0eb3-2a60-436c-9dd8-3bbca42ed4e4","team":"00000000-0000-4000-8000-00000000beef","expected_revision":17,"op":"handout_intent","node":2,"member":"m1","effect_request_id":"d2456c7d-e25e-4fe5-a6a0-3a2b247f057c"}','{"Success":{"ok":true,"revision":18,"node":2,"item":2}}');
INSERT INTO requests VALUES('bb93cfae-0c66-498a-8a53-7223eaa4fe90','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"bb93cfae-0c66-498a-8a53-7223eaa4fe90","team":"00000000-0000-4000-8000-00000000beef","expected_revision":18,"op":"handout_outcome","effect_request_id":"d2456c7d-e25e-4fe5-a6a0-3a2b247f057c","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":19}}');
INSERT INTO requests VALUES('9b62c3dc-e451-49b2-9a0d-2c11663e65ae','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"9b62c3dc-e451-49b2-9a0d-2c11663e65ae","team":"00000000-0000-4000-8000-00000000beef","expected_revision":19,"op":"observe_task","node":2,"observation":{"revision":"token-5","previous_revision":"token-4","stage":"in_progress","cancelled":false,"rank":"a00000002","title":"Task 2","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":20}}');
INSERT INTO requests VALUES('6614883d-c873-4ea1-910f-f12749510503','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"6614883d-c873-4ea1-910f-f12749510503","team":"00000000-0000-4000-8000-00000000beef","expected_revision":20,"op":"observe_task","node":2,"observation":{"revision":"token-6","previous_revision":"token-5","stage":"in_review","cancelled":false,"rank":"a00000002","title":"Task 2","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":21}}');
INSERT INTO requests VALUES('04f181cd-b10d-4d16-8af3-6da7847628f2','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"04f181cd-b10d-4d16-8af3-6da7847628f2","team":"00000000-0000-4000-8000-00000000beef","expected_revision":21,"op":"record_report","node":2,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":22,"item":3}}');
INSERT INTO requests VALUES('c02083f7-3af0-4e37-9301-bf74703699a8','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"c02083f7-3af0-4e37-9301-bf74703699a8","team":"00000000-0000-4000-8000-00000000beef","expected_revision":22,"op":"hold","node":2}','{"Success":{"ok":true,"revision":23}}');
INSERT INTO requests VALUES('4e9a950c-75a6-4ba6-8aee-a11724319a03','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"4e9a950c-75a6-4ba6-8aee-a11724319a03","team":"00000000-0000-4000-8000-00000000beef","expected_revision":23,"op":"cancel","node":2,"reason":"Cancel for the legacy hold"}','{"Success":{"ok":true,"revision":24}}');
INSERT INTO requests VALUES('e6ce526f-a641-441e-9f20-23f894ab2867','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"e6ce526f-a641-441e-9f20-23f894ab2867","team":"00000000-0000-4000-8000-00000000beef","expected_revision":24,"op":"record_cancel","node":2,"decision":2,"state":"refused","reason":"Still running","attempt":0,"attempt_id":""}','{"Success":{"ok":true,"revision":25,"node":2,"item":4}}');
INSERT INTO requests VALUES('3ed52fe2-c6ef-4c0c-b9f9-408b0ee3ba70','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"3ed52fe2-c6ef-4c0c-b9f9-408b0ee3ba70","team":"00000000-0000-4000-8000-00000000beef","expected_revision":25,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000003"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":26,"node":3}}');
INSERT INTO requests VALUES('60df581b-adb8-4d2e-8627-26868f0d5302','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"60df581b-adb8-4d2e-8627-26868f0d5302","team":"00000000-0000-4000-8000-00000000beef","expected_revision":26,"op":"observe_task","node":3,"observation":{"revision":"token-7","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000003","title":"Task 3","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":27}}');
INSERT INTO requests VALUES('e3fef2c9-911c-4745-bee5-63be9347c34f','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"e3fef2c9-911c-4745-bee5-63be9347c34f","team":"00000000-0000-4000-8000-00000000beef","expected_revision":27,"op":"hold","node":3}','{"Success":{"ok":true,"revision":28}}');
INSERT INTO requests VALUES('aad51046-3e24-4b27-8f75-67c9bfe08059','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"aad51046-3e24-4b27-8f75-67c9bfe08059","team":"00000000-0000-4000-8000-00000000beef","expected_revision":28,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000004"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":29,"node":4}}');
INSERT INTO requests VALUES('646a3012-4d7d-466f-b981-482725a1085b','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"646a3012-4d7d-466f-b981-482725a1085b","team":"00000000-0000-4000-8000-00000000beef","expected_revision":29,"op":"observe_task","node":4,"observation":{"revision":"token-8","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000004","title":"Task 4","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":30}}');
INSERT INTO requests VALUES('5bc8b0b4-0cf5-4e3a-9049-59272f8310c7','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"5bc8b0b4-0cf5-4e3a-9049-59272f8310c7","team":"00000000-0000-4000-8000-00000000beef","expected_revision":30,"op":"handout_intent","node":4,"member":"m1","effect_request_id":"a0ad0e3b-94bf-451e-bec5-ced2afa472df"}','{"Success":{"ok":true,"revision":31,"node":4,"item":3}}');
INSERT INTO requests VALUES('3d38072d-f7d0-4adc-a56a-2c49630dde5b','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"3d38072d-f7d0-4adc-a56a-2c49630dde5b","team":"00000000-0000-4000-8000-00000000beef","expected_revision":31,"op":"handout_outcome","effect_request_id":"a0ad0e3b-94bf-451e-bec5-ced2afa472df","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":32}}');
INSERT INTO requests VALUES('5a31439b-59dd-4597-84ea-ccabb4083881','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"5a31439b-59dd-4597-84ea-ccabb4083881","team":"00000000-0000-4000-8000-00000000beef","expected_revision":32,"op":"observe_task","node":4,"observation":{"revision":"token-9","previous_revision":"token-8","stage":"in_progress","cancelled":false,"rank":"a00000004","title":"Task 4","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":33}}');
INSERT INTO requests VALUES('81092f08-784e-43ae-a3a4-41e6529d0203','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"81092f08-784e-43ae-a3a4-41e6529d0203","team":"00000000-0000-4000-8000-00000000beef","expected_revision":33,"op":"observe_task","node":4,"observation":{"revision":"token-10","previous_revision":"token-9","stage":"in_review","cancelled":false,"rank":"a00000004","title":"Task 4","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":34}}');
INSERT INTO requests VALUES('d40ac0d6-f479-423b-8ae2-c41a95279e92','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"d40ac0d6-f479-423b-8ae2-c41a95279e92","team":"00000000-0000-4000-8000-00000000beef","expected_revision":34,"op":"record_report","node":4,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":35,"item":5}}');
INSERT INTO requests VALUES('85a79d23-8d75-4893-8438-3e8f84f4db4f','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"85a79d23-8d75-4893-8438-3e8f84f4db4f","team":"00000000-0000-4000-8000-00000000beef","expected_revision":35,"op":"cancel","node":4,"reason":"Cancel for the legacy hold"}','{"Success":{"ok":true,"revision":36}}');
INSERT INTO requests VALUES('b3423f5a-4a75-4585-8e63-c4f268782bcf','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"b3423f5a-4a75-4585-8e63-c4f268782bcf","team":"00000000-0000-4000-8000-00000000beef","expected_revision":36,"op":"record_cancel","node":4,"decision":3,"state":"refused","reason":"Still running","attempt":0,"attempt_id":""}','{"Success":{"ok":true,"revision":37,"node":4,"item":6}}');
INSERT INTO requests VALUES('9b5981d2-12c7-4f5b-9477-a34d83eb93d5','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"9b5981d2-12c7-4f5b-9477-a34d83eb93d5","team":"00000000-0000-4000-8000-00000000beef","expected_revision":37,"op":"release","node":4}','{"Success":{"ok":true,"revision":38}}');
INSERT INTO requests VALUES('1607f6d1-fd4f-40e0-9b0c-3313b035d78e','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"1607f6d1-fd4f-40e0-9b0c-3313b035d78e","team":"00000000-0000-4000-8000-00000000beef","expected_revision":38,"op":"hold","node":4}','{"Success":{"ok":true,"revision":39}}');
INSERT INTO requests VALUES('159fc2a4-6844-42da-a08a-6349356d8184','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"159fc2a4-6844-42da-a08a-6349356d8184","team":"00000000-0000-4000-8000-00000000beef","expected_revision":39,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000005"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":40,"node":5}}');
INSERT INTO requests VALUES('9fd74a6b-928a-40d2-b4fc-bcc16631772c','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"9fd74a6b-928a-40d2-b4fc-bcc16631772c","team":"00000000-0000-4000-8000-00000000beef","expected_revision":40,"op":"observe_task","node":5,"observation":{"revision":"token-11","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000005","title":"Task 5","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":41}}');
INSERT INTO requests VALUES('1c75f63d-ab4b-492b-9c30-ff1eeccc509e','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"1c75f63d-ab4b-492b-9c30-ff1eeccc509e","team":"00000000-0000-4000-8000-00000000beef","expected_revision":41,"op":"handout_intent","node":5,"member":"m1","effect_request_id":"ceb5ecbb-207a-4837-91ad-86ff905163fa"}','{"Success":{"ok":true,"revision":42,"node":5,"item":4}}');
INSERT INTO requests VALUES('3a54bb77-43b7-40f8-bd8f-35e41fc3af43','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"3a54bb77-43b7-40f8-bd8f-35e41fc3af43","team":"00000000-0000-4000-8000-00000000beef","expected_revision":42,"op":"handout_outcome","effect_request_id":"ceb5ecbb-207a-4837-91ad-86ff905163fa","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":43}}');
INSERT INTO requests VALUES('dcd185cf-085b-404f-84b5-05181c866c6d','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"dcd185cf-085b-404f-84b5-05181c866c6d","team":"00000000-0000-4000-8000-00000000beef","expected_revision":43,"op":"observe_task","node":5,"observation":{"revision":"token-12","previous_revision":"token-11","stage":"in_progress","cancelled":false,"rank":"a00000005","title":"Task 5","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":44}}');
INSERT INTO requests VALUES('7f10c246-9166-4127-94bf-6ba645d5130d','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"7f10c246-9166-4127-94bf-6ba645d5130d","team":"00000000-0000-4000-8000-00000000beef","expected_revision":44,"op":"observe_task","node":5,"observation":{"revision":"token-13","previous_revision":"token-12","stage":"in_review","cancelled":false,"rank":"a00000005","title":"Task 5","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":45}}');
INSERT INTO requests VALUES('6daafa1c-1596-4b84-8972-7574c89ed345','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"6daafa1c-1596-4b84-8972-7574c89ed345","team":"00000000-0000-4000-8000-00000000beef","expected_revision":45,"op":"record_report","node":5,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":46,"item":7}}');
INSERT INTO requests VALUES('3209e7a5-5911-4e98-84f8-0eb9c4a64061','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"3209e7a5-5911-4e98-84f8-0eb9c4a64061","team":"00000000-0000-4000-8000-00000000beef","expected_revision":46,"op":"return","node":5,"reason":"Reopen for the legacy hold"}','{"Success":{"ok":true,"revision":47}}');
INSERT INTO requests VALUES('137e5f4c-adce-4eb1-bfff-2cbbad98ae94','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"137e5f4c-adce-4eb1-bfff-2cbbad98ae94","team":"00000000-0000-4000-8000-00000000beef","expected_revision":47,"op":"reopen_intent","node":5,"member":"m1","effect_request_id":"85f6ee47-fec1-4ce3-9cf8-9b0421bffd42"}','{"Success":{"ok":true,"revision":48,"node":5,"item":5}}');
INSERT INTO requests VALUES('50363d9b-d98c-4e55-96f7-2368cc9b425e','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"50363d9b-d98c-4e55-96f7-2368cc9b425e","team":"00000000-0000-4000-8000-00000000beef","expected_revision":48,"op":"reopen_outcome","effect_request_id":"85f6ee47-fec1-4ce3-9cf8-9b0421bffd42","outcome":{"status":"refused","reason":"Not delivered"}}','{"Success":{"ok":true,"revision":49}}');
INSERT INTO requests VALUES('dc14676e-3b74-423c-b478-f7d58dbf657a','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"dc14676e-3b74-423c-b478-f7d58dbf657a","team":"00000000-0000-4000-8000-00000000beef","expected_revision":49,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000006"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":50,"node":6}}');
INSERT INTO requests VALUES('df392e11-c8c5-499a-9f80-66090f304e72','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"df392e11-c8c5-499a-9f80-66090f304e72","team":"00000000-0000-4000-8000-00000000beef","expected_revision":50,"op":"observe_task","node":6,"observation":{"revision":"token-14","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000006","title":"Task 6","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":51}}');
INSERT INTO requests VALUES('59af276b-4ca3-4fc1-88b1-475c6714b1ea','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"59af276b-4ca3-4fc1-88b1-475c6714b1ea","team":"00000000-0000-4000-8000-00000000beef","expected_revision":51,"op":"handout_intent","node":6,"member":"m1","effect_request_id":"9fd55718-fd04-4655-8b80-e1aaeddf6fd2"}','{"Success":{"ok":true,"revision":52,"node":6,"item":6}}');
INSERT INTO requests VALUES('4865a1a3-25ca-4230-9cf0-74223865eb74','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"4865a1a3-25ca-4230-9cf0-74223865eb74","team":"00000000-0000-4000-8000-00000000beef","expected_revision":52,"op":"handout_outcome","effect_request_id":"9fd55718-fd04-4655-8b80-e1aaeddf6fd2","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":53}}');
INSERT INTO requests VALUES('abf260b3-5438-46f8-8a4e-f1de6a7871da','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"abf260b3-5438-46f8-8a4e-f1de6a7871da","team":"00000000-0000-4000-8000-00000000beef","expected_revision":53,"op":"observe_task","node":6,"observation":{"revision":"token-15","previous_revision":"token-14","stage":"in_progress","cancelled":false,"rank":"a00000006","title":"Task 6","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":54}}');
INSERT INTO requests VALUES('78f50112-815a-4a3d-8324-118075a126ad','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"78f50112-815a-4a3d-8324-118075a126ad","team":"00000000-0000-4000-8000-00000000beef","expected_revision":54,"op":"observe_task","node":6,"observation":{"revision":"token-16","previous_revision":"token-15","stage":"in_review","cancelled":false,"rank":"a00000006","title":"Task 6","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":55}}');
INSERT INTO requests VALUES('29bc35e6-d622-43c5-ac4d-11d170f32ceb','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"29bc35e6-d622-43c5-ac4d-11d170f32ceb","team":"00000000-0000-4000-8000-00000000beef","expected_revision":55,"op":"record_report","node":6,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":56,"item":8}}');
INSERT INTO requests VALUES('7d658e93-3fe6-477c-b19b-5770cdee6b82','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"7d658e93-3fe6-477c-b19b-5770cdee6b82","team":"00000000-0000-4000-8000-00000000beef","expected_revision":56,"op":"return","node":6,"reason":"Reopen for the legacy hold"}','{"Success":{"ok":true,"revision":57}}');
INSERT INTO requests VALUES('5a9ed9ff-cc38-4d48-8c27-c70dccb31e9b','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"5a9ed9ff-cc38-4d48-8c27-c70dccb31e9b","team":"00000000-0000-4000-8000-00000000beef","expected_revision":57,"op":"reopen_intent","node":6,"member":"m1","effect_request_id":"4730d997-9033-4f38-9b72-6435ac8c0f96"}','{"Success":{"ok":true,"revision":58,"node":6,"item":7}}');
INSERT INTO requests VALUES('bfa53685-4ef4-45b0-a47e-2509eeb1b6ba','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"bfa53685-4ef4-45b0-a47e-2509eeb1b6ba","team":"00000000-0000-4000-8000-00000000beef","expected_revision":58,"op":"reopen_outcome","effect_request_id":"4730d997-9033-4f38-9b72-6435ac8c0f96","outcome":{"status":"cutting_over","reason":"Replacement starting"}}','{"Success":{"ok":true,"revision":59,"item":9}}');
INSERT INTO requests VALUES('b1f9be4e-591e-4006-a263-ecb64d227d33','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"b1f9be4e-591e-4006-a263-ecb64d227d33","team":"00000000-0000-4000-8000-00000000beef","expected_revision":59,"op":"release","node":6}','{"Success":{"ok":true,"revision":60}}');
INSERT INTO requests VALUES('ae9142bc-1897-4209-b183-be63303c7714','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"ae9142bc-1897-4209-b183-be63303c7714","team":"00000000-0000-4000-8000-00000000beef","expected_revision":60,"op":"hold","node":6}','{"Success":{"ok":true,"revision":61}}');
INSERT INTO requests VALUES('3b602bbf-0e81-440b-bd34-2130a42d4606','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"3b602bbf-0e81-440b-bd34-2130a42d4606","team":"00000000-0000-4000-8000-00000000beef","expected_revision":61,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000007"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":62,"node":7}}');
INSERT INTO requests VALUES('67478ad2-e297-41a5-945c-d92d4c0698e9','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"67478ad2-e297-41a5-945c-d92d4c0698e9","team":"00000000-0000-4000-8000-00000000beef","expected_revision":62,"op":"observe_task","node":7,"observation":{"revision":"token-17","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000007","title":"Task 7","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":63}}');
INSERT INTO requests VALUES('af1e2a8e-8a00-490a-be2c-e0bce2d7c23f','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"af1e2a8e-8a00-490a-be2c-e0bce2d7c23f","team":"00000000-0000-4000-8000-00000000beef","expected_revision":63,"op":"hold","node":7}','{"Success":{"ok":true,"revision":64}}');
CREATE TABLE pipeline_runs (
 team TEXT NOT NULL, node INTEGER NOT NULL, generation INTEGER NOT NULL,
 state TEXT NOT NULL, stage INTEGER,
 PRIMARY KEY(team,node), FOREIGN KEY(team,node) REFERENCES node_identity(team,id)
);
INSERT INTO pipeline_runs VALUES('00000000-0000-4000-8000-00000000beef',1,1,'overridden',NULL);
INSERT INTO pipeline_runs VALUES('00000000-0000-4000-8000-00000000beef',2,1,'overridden',NULL);
INSERT INTO pipeline_runs VALUES('00000000-0000-4000-8000-00000000beef',4,1,'overridden',NULL);
INSERT INTO pipeline_runs VALUES('00000000-0000-4000-8000-00000000beef',5,1,'overridden',NULL);
INSERT INTO pipeline_runs VALUES('00000000-0000-4000-8000-00000000beef',6,1,'overridden',NULL);
CREATE TABLE stage_tasks (
 team TEXT NOT NULL, node INTEGER NOT NULL, target INTEGER NOT NULL,
 generation INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'wanted',
 request_id TEXT UNIQUE, outcome TEXT, created_ms INTEGER NOT NULL,
 PRIMARY KEY(team,node), FOREIGN KEY(team,node) REFERENCES node_identity(team,id)
);
CREATE TABLE verdicts (
 team TEXT NOT NULL, node INTEGER NOT NULL, generation INTEGER NOT NULL,
 actor TEXT NOT NULL, pass INTEGER NOT NULL, text TEXT NOT NULL,
 PRIMARY KEY(team,node,generation), FOREIGN KEY(team,node) REFERENCES node_identity(team,id)
);
CREATE TABLE task_observations (
 team TEXT NOT NULL, node INTEGER NOT NULL, data TEXT NOT NULL,
 released INTEGER NOT NULL DEFAULT 0,
 attempt INTEGER NOT NULL DEFAULT 0, attempt_id TEXT NOT NULL DEFAULT '',
 last_attempt_id TEXT NOT NULL DEFAULT '', attempt_conflict INTEGER NOT NULL DEFAULT 0,
 PRIMARY KEY(team,node), FOREIGN KEY(team,node) REFERENCES node_identity(team,id)
);
INSERT INTO task_observations VALUES('00000000-0000-4000-8000-00000000beef',1,'{"revision":"token-3","previous_revision":"token-2","stage":"in_review","cancelled":false,"rank":"a00000001","title":"Task 1","attempt":0,"attempt_id":"","attempt_conflict":false}',0,0,'','',0);
INSERT INTO task_observations VALUES('00000000-0000-4000-8000-00000000beef',2,'{"revision":"token-6","previous_revision":"token-5","stage":"in_review","cancelled":false,"rank":"a00000002","title":"Task 2","attempt":0,"attempt_id":"","attempt_conflict":false}',0,0,'','',0);
INSERT INTO task_observations VALUES('00000000-0000-4000-8000-00000000beef',3,'{"revision":"token-7","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000003","title":"Task 3","attempt":0,"attempt_id":"","attempt_conflict":false}',0,0,'','',0);
INSERT INTO task_observations VALUES('00000000-0000-4000-8000-00000000beef',4,'{"revision":"token-10","previous_revision":"token-9","stage":"in_review","cancelled":false,"rank":"a00000004","title":"Task 4","attempt":0,"attempt_id":"","attempt_conflict":false}',0,0,'','',0);
INSERT INTO task_observations VALUES('00000000-0000-4000-8000-00000000beef',5,'{"revision":"token-13","previous_revision":"token-12","stage":"in_review","cancelled":false,"rank":"a00000005","title":"Task 5","attempt":0,"attempt_id":"","attempt_conflict":false}',0,0,'','',0);
INSERT INTO task_observations VALUES('00000000-0000-4000-8000-00000000beef',6,'{"revision":"token-16","previous_revision":"token-15","stage":"in_review","cancelled":false,"rank":"a00000006","title":"Task 6","attempt":0,"attempt_id":"","attempt_conflict":false}',0,0,'','',0);
INSERT INTO task_observations VALUES('00000000-0000-4000-8000-00000000beef',7,'{"revision":"token-17","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000007","title":"Task 7","attempt":0,"attempt_id":"","attempt_conflict":false}',0,0,'','',0);
CREATE TABLE timers_extension (id INTEGER PRIMARY KEY CHECK(id=1), version INTEGER NOT NULL);
INSERT INTO timers_extension VALUES(1,10);
CREATE TABLE key_clear_teams (team TEXT PRIMARY KEY, pending INTEGER NOT NULL);
CREATE TABLE key_clear_work (
 team TEXT NOT NULL, node INTEGER NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('stage','stop')),
 PRIMARY KEY(team,node)
);
CREATE TABLE timer_firings (
 team TEXT NOT NULL, kind TEXT NOT NULL, node INTEGER NOT NULL, generation INTEGER NOT NULL,
 item INTEGER NOT NULL, target TEXT NOT NULL CHECK(target IN ('inbox','card')),
 seq INTEGER NOT NULL, subject INTEGER NOT NULL, created_ms INTEGER NOT NULL,
 PRIMARY KEY(team,kind,node,generation,item)
);
CREATE TABLE effect_outcomes (
 team TEXT NOT NULL, request_id TEXT NOT NULL, at_ms INTEGER NOT NULL,
 PRIMARY KEY(team,request_id)
);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','224ecd68-4b3e-49b1-8336-019148e68502',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','d2456c7d-e25e-4fe5-a6a0-3a2b247f057c',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','a0ad0e3b-94bf-451e-bec5-ced2afa472df',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','ceb5ecbb-207a-4837-91ad-86ff905163fa',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','85f6ee47-fec1-4ce3-9cf8-9b0421bffd42',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','9fd55718-fd04-4655-8b80-e1aaeddf6fd2',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','4730d997-9033-4f38-9b72-6435ac8c0f96',1234);
CREATE TABLE key_clears (
 team TEXT NOT NULL, node INTEGER NOT NULL, state TEXT NOT NULL CHECK(state IN ('cleared','refused')),
 reason TEXT NOT NULL, at_ms INTEGER NOT NULL,
 PRIMARY KEY(team,node), FOREIGN KEY(team,node) REFERENCES node_identity(team,id)
);
CREATE TABLE late_sessions (
 team TEXT NOT NULL, member TEXT NOT NULL, launch TEXT NOT NULL, machine TEXT NOT NULL,
 session_id TEXT NOT NULL, name TEXT NOT NULL, card INTEGER NOT NULL, created_ms INTEGER NOT NULL,
 PRIMARY KEY(team,member,launch)
);
CREATE TABLE cancel_outcomes (
 decision INTEGER PRIMARY KEY, team TEXT NOT NULL, node INTEGER NOT NULL, generation INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('cancelled','refused','released')), reason TEXT NOT NULL, at_ms INTEGER NOT NULL
);
INSERT INTO cancel_outcomes VALUES(1,'00000000-0000-4000-8000-00000000beef',1,1,'refused','Still running',1234);
INSERT INTO cancel_outcomes VALUES(2,'00000000-0000-4000-8000-00000000beef',2,1,'refused','Still running',1234);
INSERT INTO cancel_outcomes VALUES(3,'00000000-0000-4000-8000-00000000beef',4,1,'refused','Still running',1234);
CREATE TABLE answer_marks (
 team TEXT NOT NULL, member TEXT NOT NULL, node INTEGER NOT NULL, generation INTEGER NOT NULL,
 revision INTEGER NOT NULL, replies INTEGER NOT NULL, PRIMARY KEY(team,member)
);
PRAGMA writable_schema=ON;
CREATE TABLE IF NOT EXISTS sqlite_sequence(name,seq);
DELETE FROM sqlite_sequence;
INSERT INTO sqlite_sequence VALUES('journal',64);
INSERT INTO sqlite_sequence VALUES('effects',7);
INSERT INTO sqlite_sequence VALUES('inbox',9);
INSERT INTO sqlite_sequence VALUES('decisions',5);
CREATE TRIGGER key_clear_terminal_v1 AFTER UPDATE OF status ON teams
 WHEN NEW.status IN ('"stopped"','"archived"') BEGIN
 INSERT OR IGNORE INTO key_clear_teams(team,pending) VALUES(NEW.id,1);
END;
CREATE TRIGGER timer_cards_decided_v1 AFTER INSERT ON decisions BEGIN
 UPDATE cards SET state='closed',decision='stale',decided_actor=actor
  WHERE state='open' AND kind='report_waiting' AND seq IN
  (SELECT seq FROM timer_firings WHERE team=NEW.team AND node=NEW.node AND target='card');
END;
CREATE TRIGGER timer_cards_item_v1 AFTER UPDATE OF state ON inbox
 WHEN OLD.state='open' AND NEW.state<>'open' BEGIN
 UPDATE cards SET state='closed',decision='stale',decided_actor=actor
  WHERE state='open' AND kind IN ('report_waiting','silent_lead') AND seq IN
  (SELECT seq FROM timer_firings WHERE team=NEW.team AND subject=NEW.seq AND target='card');
END;
CREATE TRIGGER key_clear_stage_archived_v2 AFTER INSERT ON node_archive
 WHEN json_extract(NEW.data,'$.kind')='stage' BEGIN
 INSERT OR IGNORE INTO key_clear_work(team,node,kind) SELECT a.team,a.id,'stage' FROM node_archive a
  WHERE a.team=NEW.team AND a.id=NEW.id AND json_extract(a.data,'$.kind')='stage'
  AND NOT EXISTS(SELECT 1 FROM stage_tasks s WHERE s.team=a.team AND s.node=a.id AND s.state IN ('wanted','reserved','created'))
  AND (EXISTS(SELECT 1 FROM node_identity i WHERE i.team=a.team AND i.id=a.id AND i.task_uuid<>'')
   OR EXISTS(SELECT 1 FROM stage_tasks s WHERE s.team=a.team AND s.node=a.id AND s.request_id IS NOT NULL))
  AND NOT EXISTS(SELECT 1 FROM key_clears k WHERE k.team=a.team AND k.node=a.id);
 UPDATE key_clear_teams SET pending=1 WHERE team=NEW.team AND pending=0
  AND EXISTS(SELECT 1 FROM key_clear_work w WHERE w.team=NEW.team AND w.node=NEW.id);
END;
CREATE TRIGGER key_clear_stage_settled_v2 AFTER UPDATE OF state ON stage_tasks
 WHEN NEW.state NOT IN ('wanted','reserved','created') BEGIN
 INSERT OR IGNORE INTO key_clear_work(team,node,kind) SELECT a.team,a.id,'stage' FROM node_archive a
  WHERE a.team=NEW.team AND a.id=NEW.node AND json_extract(a.data,'$.kind')='stage'
  AND NOT EXISTS(SELECT 1 FROM stage_tasks s WHERE s.team=a.team AND s.node=a.id AND s.state IN ('wanted','reserved','created'))
  AND (EXISTS(SELECT 1 FROM node_identity i WHERE i.team=a.team AND i.id=a.id AND i.task_uuid<>'')
   OR EXISTS(SELECT 1 FROM stage_tasks s WHERE s.team=a.team AND s.node=a.id AND s.request_id IS NOT NULL))
  AND NOT EXISTS(SELECT 1 FROM key_clears k WHERE k.team=a.team AND k.node=a.id);
 UPDATE key_clear_teams SET pending=1 WHERE team=NEW.team AND pending=0
  AND EXISTS(SELECT 1 FROM key_clear_work w WHERE w.team=NEW.team AND w.node=NEW.node);
END;
CREATE TRIGGER key_clear_cancel_settled_v1 AFTER UPDATE OF state ON decisions
 WHEN NEW.kind='cancel' AND OLD.state<>'settled' AND NEW.state='settled' BEGIN
 INSERT OR IGNORE INTO key_clear_work(team,node,kind) SELECT i.team,i.id,'stop' FROM node_identity i
  LEFT JOIN stage_tasks s ON s.team=i.team AND s.node=i.id
  WHERE i.team=NEW.team AND i.id=NEW.node
  AND EXISTS(SELECT 1 FROM teams t WHERE t.id=NEW.team AND t.status IN ('"stopped"','"archived"'))
  AND (i.task_uuid<>'' OR s.request_id IS NOT NULL)
  AND NOT EXISTS(SELECT 1 FROM task_observations o WHERE o.team=i.team AND o.node=i.id AND json_extract(o.data,'$.key') IS NOT NULL)
  AND NOT EXISTS(SELECT 1 FROM key_clears k WHERE k.team=i.team AND k.node=i.id);
END;
CREATE TRIGGER answer_mark_drop_v1 AFTER INSERT ON decisions BEGIN
 DELETE FROM answer_marks WHERE team=NEW.team AND node=NEW.node;
END;
CREATE INDEX member_session ON members(team,machine,session_id);
CREATE INDEX identity_task ON node_identity(task_project,task_uuid,team,id);
CREATE INDEX live_tasks_team ON live_tasks(team,node);
CREATE INDEX queue_rank ON nodes(team,rank,id);
CREATE INDEX nodes_state ON nodes(team,node_state,id);
CREATE UNIQUE INDEX archive_node ON node_archive(team,id);
CREATE INDEX archive_state ON node_archive(team,node_state,id);
CREATE INDEX dependency_dependants ON dependencies(team,dependency,node);
CREATE INDEX inbox_page ON inbox(team,seq);
CREATE INDEX inbox_open ON inbox(team,state,seq);
CREATE INDEX cards_page ON cards(team,seq);
CREATE INDEX cards_open ON cards(team,state,seq);
CREATE INDEX journal_page ON journal(team,seq);
CREATE UNIQUE INDEX journal_archive_page ON journal_archive(team,seq);
CREATE INDEX effects_page ON effects(team,seq);
CREATE INDEX effects_pending ON effects(team,state,seq);
CREATE UNIQUE INDEX effect_node ON effects(team,node) WHERE state IN ('reserved','submitted','uncertain','cutting_over');
CREATE UNIQUE INDEX effect_member ON effects(team,member) WHERE state IN ('reserved','submitted','uncertain','cutting_over');
CREATE INDEX decisions_node ON decisions(team,node,seq);
CREATE INDEX decisions_page ON decisions(team,seq);
CREATE UNIQUE INDEX decision_pending ON decisions(team,node) WHERE state<>'settled';
CREATE INDEX integration_node ON integration_facts(team,node,decision);
CREATE INDEX stage_tasks_pending ON stage_tasks(team,node)
 WHERE state IN ('wanted','reserved');
CREATE INDEX teams_live ON teams(id)
 WHERE status IN ('"running"','"paused"');
CREATE INDEX cards_waiting ON cards(seq) WHERE state='open';
CREATE INDEX effects_attempt ON effects(team,node,generation,seq);
CREATE INDEX integration_approved ON cards(team,decision_seq)
 WHERE kind='integration' AND state='closed' AND decision='approve';
CREATE INDEX integration_card ON cards(decision_seq)
 WHERE kind='integration';
CREATE INDEX key_clear_pending ON key_clear_teams(team) WHERE pending=1;
CREATE INDEX key_clear_work_kind ON key_clear_work(team,kind,node);
CREATE INDEX timer_card_subject ON timer_firings(team,subject) WHERE target='card';
CREATE INDEX timer_card_node ON timer_firings(team,node) WHERE target='card';
CREATE INDEX key_refusals ON key_clears(team,node) WHERE state='refused';
CREATE INDEX cancel_refusals ON cancel_outcomes(team,node) WHERE state='refused';
CREATE INDEX decisions_attempt ON decisions(team,generation,node);
CREATE INDEX answer_marks_node ON answer_marks(team,node);
CREATE INDEX inbox_questions ON inbox(team,node,generation,seq) WHERE kind GLOB 'question:*';
PRAGMA writable_schema=OFF;
COMMIT;
PRAGMA user_version=2;
