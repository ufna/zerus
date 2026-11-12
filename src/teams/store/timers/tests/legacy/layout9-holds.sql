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
 state TEXT NOT NULL DEFAULT 'open', decision TEXT, decided_actor TEXT, created_ms INTEGER NOT NULL, decision_seq INTEGER);
CREATE TABLE journal (seq INTEGER PRIMARY KEY AUTOINCREMENT, team TEXT NOT NULL REFERENCES teams(id),
 at_ms INTEGER NOT NULL, actor TEXT NOT NULL, kind TEXT NOT NULL, request_id TEXT NOT NULL, text TEXT NOT NULL,
 refs TEXT NOT NULL DEFAULT '[]');
INSERT INTO journal VALUES(1,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','create','8acf42f3-8e1c-43df-9fa0-f7f0317b59ea','create at revision 1','["team:00000000-0000-4000-8000-00000000beef"]');
INSERT INTO journal VALUES(2,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','set_member_session','726e66a7-7857-4d98-9535-cde4290e20f9','set_member_session at revision 2','["team:00000000-0000-4000-8000-00000000beef","member:lead"]');
INSERT INTO journal VALUES(3,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','set_member_session','682cd232-2b6a-4852-8cd9-0f531cc256d7','set_member_session at revision 3','["team:00000000-0000-4000-8000-00000000beef","member:m1"]');
INSERT INTO journal VALUES(4,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','set_member_session','a3a78651-582b-4007-9ae5-99895cbb3a1d','set_member_session at revision 4','["team:00000000-0000-4000-8000-00000000beef","member:m2"]');
INSERT INTO journal VALUES(5,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','082780e9-e89f-46a7-8b14-a67586657acd','add_task at revision 5','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000001"]');
INSERT INTO journal VALUES(6,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','bb767772-7106-4a30-a4b9-686fcfca350e','observe_task at revision 6','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(7,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','3b2967c8-e0d5-4a26-82ac-a8c0ce0b0953','handout_intent at revision 7','["team:00000000-0000-4000-8000-00000000beef","node:1","member:m1","effect:bbb56746-dbd9-4662-99c1-0527bacddc23"]');
INSERT INTO journal VALUES(8,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','7aef2361-db5b-41bd-b2c6-dcecc55f809b','handout_outcome at revision 8','["team:00000000-0000-4000-8000-00000000beef","effect:bbb56746-dbd9-4662-99c1-0527bacddc23"]');
INSERT INTO journal VALUES(9,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','c90cab28-2a65-4934-88e7-1dce320c0e2c','observe_task at revision 9','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(10,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','7407e333-171d-43f7-b31c-a7ebfc003694','observe_task at revision 10','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(11,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','1ad54dc2-84f0-4da9-bc1e-9d38b555f343','record_report at revision 11','["team:00000000-0000-4000-8000-00000000beef","node:1","member:m1"]');
INSERT INTO journal VALUES(12,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','6f93c2ee-278e-408f-abfc-ce9bba5c8ace','cancel at revision 12','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(13,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_cancel','dd5f3af6-be88-4195-823f-24c502cf1b7d','record_cancel at revision 13','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(14,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','release','567bd974-7ed8-41f3-be44-e06b67f5420c','release at revision 14','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(15,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','09a941e5-64c3-4381-81bb-77fb707dfabb','hold at revision 15','["team:00000000-0000-4000-8000-00000000beef","node:1"]');
INSERT INTO journal VALUES(16,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','35e782b8-3299-49bf-80b6-4fd732f7ed67','add_task at revision 16','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000002"]');
INSERT INTO journal VALUES(17,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','e7e7eeb5-1083-4c91-9e8a-8b5fe8ed6ea6','observe_task at revision 17','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(18,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','cfc51ecc-00a7-4725-809a-5aaa4059f21e','handout_intent at revision 18','["team:00000000-0000-4000-8000-00000000beef","node:2","member:m1","effect:8a7dc696-55e9-4104-84e8-7d74f8db8029"]');
INSERT INTO journal VALUES(19,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','00ad0cd0-40c5-49af-b38b-937b16dff5cc','handout_outcome at revision 19','["team:00000000-0000-4000-8000-00000000beef","effect:8a7dc696-55e9-4104-84e8-7d74f8db8029"]');
INSERT INTO journal VALUES(20,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','66e3ce64-4e80-4000-934e-33972a88b205','observe_task at revision 20','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(21,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','02360aee-8167-4ac2-90eb-6d25a7648ab4','observe_task at revision 21','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(22,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','01bcabda-ad03-42d7-894b-911f1461e914','record_report at revision 22','["team:00000000-0000-4000-8000-00000000beef","node:2","member:m1"]');
INSERT INTO journal VALUES(23,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','655a1185-a31d-4c43-8f59-7a361b6758a7','hold at revision 23','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(24,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','c0b069fc-ca59-49c2-98d3-f18fe2e7eb4a','cancel at revision 24','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(25,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_cancel','b83b5994-1066-449e-acb5-3feebcb4494c','record_cancel at revision 25','["team:00000000-0000-4000-8000-00000000beef","node:2"]');
INSERT INTO journal VALUES(26,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','284b6868-f7ea-4279-9279-331108398c46','add_task at revision 26','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000003"]');
INSERT INTO journal VALUES(27,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','1ffa3adf-ee26-4dda-8a59-2c4a9912c4d9','observe_task at revision 27','["team:00000000-0000-4000-8000-00000000beef","node:3"]');
INSERT INTO journal VALUES(28,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','hold','454d5ff9-d02b-40d5-bc86-2564e881068d','hold at revision 28','["team:00000000-0000-4000-8000-00000000beef","node:3"]');
INSERT INTO journal VALUES(29,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','3dd0f36c-f087-4e7b-9b65-25eb0d577019','add_task at revision 29','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000004"]');
INSERT INTO journal VALUES(30,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','ae407eb8-07c6-49c8-bb01-7e10b87f450f','observe_task at revision 30','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(31,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','0e3bf13a-855a-457d-9795-54c73e3bfbb4','handout_intent at revision 31','["team:00000000-0000-4000-8000-00000000beef","node:4","member:m1","effect:dc2dd42c-e015-4373-a7d3-229df2bc2ab9"]');
INSERT INTO journal VALUES(32,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','651e7c51-3aad-460f-8a5d-d67d0f4c6afc','handout_outcome at revision 32','["team:00000000-0000-4000-8000-00000000beef","effect:dc2dd42c-e015-4373-a7d3-229df2bc2ab9"]');
INSERT INTO journal VALUES(33,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','cfde9455-2a31-4941-b6ad-fe3bc2883d59','observe_task at revision 33','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(34,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','02cf0c9e-e88e-4103-93de-0bcdb72fe033','observe_task at revision 34','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(35,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','ae50aff6-4383-4f99-b351-c1b1b5369302','record_report at revision 35','["team:00000000-0000-4000-8000-00000000beef","node:4","member:m1"]');
INSERT INTO journal VALUES(36,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','cancel','156b13f8-d31a-47f7-b0a0-4f3a67e1fbb2','cancel at revision 36','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(37,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_cancel','af675923-0343-412e-88fd-4c93b32a58fa','record_cancel at revision 37','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(38,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','release','abc68d63-a1f0-407a-a51f-36dddf87aea1','release at revision 38','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(39,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','hold','ebfa01b8-d95e-4d36-bf07-e2e135cfb06a','hold at revision 39','["team:00000000-0000-4000-8000-00000000beef","node:4"]');
INSERT INTO journal VALUES(40,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','771a9cf6-e3f5-4812-bcea-9ec809d21612','add_task at revision 40','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000005"]');
INSERT INTO journal VALUES(41,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','a518fc37-d2d3-4d4f-b7e9-863483d2cc3a','observe_task at revision 41','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(42,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','cbb0a854-0891-4425-94d2-fe8b478ccac2','handout_intent at revision 42','["team:00000000-0000-4000-8000-00000000beef","node:5","member:m1","effect:0e0b03c6-52f3-4d2c-9da9-d0b222a075e3"]');
INSERT INTO journal VALUES(43,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','9b915b70-97da-487f-88a0-a0200b3c038c','handout_outcome at revision 43','["team:00000000-0000-4000-8000-00000000beef","effect:0e0b03c6-52f3-4d2c-9da9-d0b222a075e3"]');
INSERT INTO journal VALUES(44,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','42885594-1ea5-49f5-9d1b-a6e0526e3299','observe_task at revision 44','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(45,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','60627a86-35b1-43e3-9946-f3e9d1da33fd','observe_task at revision 45','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(46,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','68bab8c4-41ed-47d4-a37e-27c71cba8fd6','record_report at revision 46','["team:00000000-0000-4000-8000-00000000beef","node:5","member:m1"]');
INSERT INTO journal VALUES(47,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','return','1754e14a-6642-408c-a8de-3733c692b6b3','return at revision 47','["team:00000000-0000-4000-8000-00000000beef","node:5"]');
INSERT INTO journal VALUES(48,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_intent','37292594-721a-479a-9b4a-311f365ce3c0','reopen_intent at revision 48','["team:00000000-0000-4000-8000-00000000beef","node:5","member:m1","effect:35f3950a-819f-4392-9349-a9264908e713"]');
INSERT INTO journal VALUES(49,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_outcome','47402f97-e59e-45d4-8f16-3f1fb60f4d45','reopen_outcome at revision 49','["team:00000000-0000-4000-8000-00000000beef","effect:35f3950a-819f-4392-9349-a9264908e713"]');
INSERT INTO journal VALUES(50,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','9f4e90ea-7c87-447a-8cac-d6b17483928d','add_task at revision 50','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000006"]');
INSERT INTO journal VALUES(51,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','7a278d23-6bfe-41b3-8f42-bcb4892d226b','observe_task at revision 51','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(52,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_intent','932ce2f6-ccaf-49c2-a48a-02ca4af6bb94','handout_intent at revision 52','["team:00000000-0000-4000-8000-00000000beef","node:6","member:m1","effect:7229a188-f5ad-40c2-837e-1ffd62b986dd"]');
INSERT INTO journal VALUES(53,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout_outcome','585147f4-1f89-41ea-abbd-7423715b37dc','handout_outcome at revision 53','["team:00000000-0000-4000-8000-00000000beef","effect:7229a188-f5ad-40c2-837e-1ffd62b986dd"]');
INSERT INTO journal VALUES(54,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','61cd97c4-e172-4983-be37-adc9c93550c8','observe_task at revision 54','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(55,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','abe866c7-52bd-4420-93e1-1af9328ed67d','observe_task at revision 55','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(56,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','record_report','cbe9a249-7f84-4ca1-a74a-4fee5eb06e27','record_report at revision 56','["team:00000000-0000-4000-8000-00000000beef","node:6","member:m1"]');
INSERT INTO journal VALUES(57,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','return','a81024b2-5441-488d-b1c9-10ea15bc5845','return at revision 57','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(58,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_intent','5c3b5660-8855-4730-9dfb-f0b1895d5fe3','reopen_intent at revision 58','["team:00000000-0000-4000-8000-00000000beef","node:6","member:m1","effect:05481723-db0e-47d5-8eb0-63ba5cf0acd2"]');
INSERT INTO journal VALUES(59,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen_outcome','f93527a5-0e76-4fd7-a528-f845ef280553','reopen_outcome at revision 59','["team:00000000-0000-4000-8000-00000000beef","effect:05481723-db0e-47d5-8eb0-63ba5cf0acd2"]');
INSERT INTO journal VALUES(60,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','release','2daf0e97-7104-4193-9bb3-4f1725442176','release at revision 60','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(61,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','a7cf26d4-468a-4aa6-8e55-735cbfa1f144','hold at revision 61','["team:00000000-0000-4000-8000-00000000beef","node:6"]');
INSERT INTO journal VALUES(62,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','add_task','0d62f863-1075-40dc-bb87-4dc0cd509264','add_task at revision 62','["team:00000000-0000-4000-8000-00000000beef","task:00000000-0000-4000-8000-000000000007"]');
INSERT INTO journal VALUES(63,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','observe_task','8a95884b-e74b-465e-a359-e2a35871d536','observe_task at revision 63','["team:00000000-0000-4000-8000-00000000beef","node:7"]');
INSERT INTO journal VALUES(64,'00000000-0000-4000-8000-00000000beef',1234,'{"kind":"person","person":"local","origin":"desktop_ui"}','hold','dafe8a2c-51de-4118-bc4d-71f7ea4c8764','hold at revision 64','["team:00000000-0000-4000-8000-00000000beef","node:7"]');
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
INSERT INTO effects VALUES(1,'00000000-0000-4000-8000-00000000beef',1,'m1',1,'bbb56746-dbd9-4662-99c1-0527bacddc23','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(2,'00000000-0000-4000-8000-00000000beef',2,'m1',1,'8a7dc696-55e9-4104-84e8-7d74f8db8029','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(3,'00000000-0000-4000-8000-00000000beef',4,'m1',1,'dc2dd42c-e015-4373-a7d3-229df2bc2ab9','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(4,'00000000-0000-4000-8000-00000000beef',5,'m1',1,'0e0b03c6-52f3-4d2c-9da9-d0b222a075e3','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(5,'00000000-0000-4000-8000-00000000beef',5,'m1',2,'35f3950a-819f-4392-9349-a9264908e713','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen','refused','{"status":"refused","reason":"Not delivered"}',1234);
INSERT INTO effects VALUES(6,'00000000-0000-4000-8000-00000000beef',6,'m1',1,'7229a188-f5ad-40c2-837e-1ffd62b986dd','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','handout','reported','{"status":"submitted","receipt":"receipt-example"}',1234);
INSERT INTO effects VALUES(7,'00000000-0000-4000-8000-00000000beef',6,'m1',2,'05481723-db0e-47d5-8eb0-63ba5cf0acd2','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','reopen','cutting_over','{"status":"cutting_over","reason":"Replacement starting"}',1234);
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
INSERT INTO requests VALUES('8acf42f3-8e1c-43df-9fa0-f7f0317b59ea','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"8acf42f3-8e1c-43df-9fa0-f7f0317b59ea","team":"00000000-0000-4000-8000-00000000beef","expected_revision":0,"op":"create","project":"example-project","name":"Fixes","goal":"","home":"00000000-0000-4000-8000-0000000000aa","bundle":{"template":{"id":"bugfix","name":"Bugfix","description":"A queue of independent fixes: a developer and a reviewer.","lead":{"agent":"claude"},"roster":[{"role":"developer","count":2},{"role":"reviewer","count":1}],"default_pipeline":"reviewed","settings":{"delegations":{"accept_results":true,"answer_members":true,"confirm_trust":true,"resume_exited":true,"merge_accepted":true},"caps":{"max_members":6,"max_phase_tasks":50,"max_open_tasks":500,"max_open_questions":3},"timers":{"report_timeout_seconds":120,"start_timeout_seconds":180,"inbox_interval_seconds":120,"remind_lead_minutes":15,"remind_person_minutes":60,"lead_stall_minutes":20,"answer_escalate_minutes":10,"resume_limit_per_hour":3},"queue":true,"routing":"direct","plan_approval":"review","phase_acceptance":"lead","integration":"branch","land_allowed":false}},"roles":[{"id":"developer","name":"Developer","prompt":"Implement the task in your own worktree. Start from the base commit named in the task, keep to its write set, run the checks that can fail for your change, commit, and report what you changed and how you checked it.","agent":"claude"},{"id":"reviewer","name":"Reviewer","prompt":"Review the reported commit against the task''s criteria. You never change what you review. Give a verdict: pass, or return with the exact findings.","agent":"claude"}],"pipelines":[{"id":"reviewed","name":"Reviewed: review, lead","implementer":"developer"}]},"integration_branch":"zerus/team/fixes","base_commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":1}}');
INSERT INTO requests VALUES('726e66a7-7857-4d98-9535-cde4290e20f9','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"726e66a7-7857-4d98-9535-cde4290e20f9","team":"00000000-0000-4000-8000-00000000beef","expected_revision":1,"op":"set_member_session","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"},"account":"account-example","worktree":"/workspace/example-team-lead","ownership_nonce":"nonce-lead"}','{"Success":{"ok":true,"revision":2}}');
INSERT INTO requests VALUES('682cd232-2b6a-4852-8cd9-0f531cc256d7','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"682cd232-2b6a-4852-8cd9-0f531cc256d7","team":"00000000-0000-4000-8000-00000000beef","expected_revision":2,"op":"set_member_session","member":"m1","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000031"},"account":"account-example","worktree":"/workspace/example-team-m1","ownership_nonce":"nonce-m1"}','{"Success":{"ok":true,"revision":3}}');
INSERT INTO requests VALUES('a3a78651-582b-4007-9ae5-99895cbb3a1d','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"a3a78651-582b-4007-9ae5-99895cbb3a1d","team":"00000000-0000-4000-8000-00000000beef","expected_revision":3,"op":"set_member_session","member":"m2","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000032"},"account":"account-example","worktree":"/workspace/example-team-m2","ownership_nonce":"nonce-m2"}','{"Success":{"ok":true,"revision":4}}');
INSERT INTO requests VALUES('082780e9-e89f-46a7-8b14-a67586657acd','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"082780e9-e89f-46a7-8b14-a67586657acd","team":"00000000-0000-4000-8000-00000000beef","expected_revision":4,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000001"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":5,"node":1}}');
INSERT INTO requests VALUES('bb767772-7106-4a30-a4b9-686fcfca350e','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"bb767772-7106-4a30-a4b9-686fcfca350e","team":"00000000-0000-4000-8000-00000000beef","expected_revision":5,"op":"observe_task","node":1,"observation":{"revision":"token-1","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000001","title":"Task 1","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":6}}');
INSERT INTO requests VALUES('3b2967c8-e0d5-4a26-82ac-a8c0ce0b0953','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"3b2967c8-e0d5-4a26-82ac-a8c0ce0b0953","team":"00000000-0000-4000-8000-00000000beef","expected_revision":6,"op":"handout_intent","node":1,"member":"m1","effect_request_id":"bbb56746-dbd9-4662-99c1-0527bacddc23"}','{"Success":{"ok":true,"revision":7,"node":1,"item":1}}');
INSERT INTO requests VALUES('7aef2361-db5b-41bd-b2c6-dcecc55f809b','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"7aef2361-db5b-41bd-b2c6-dcecc55f809b","team":"00000000-0000-4000-8000-00000000beef","expected_revision":7,"op":"handout_outcome","effect_request_id":"bbb56746-dbd9-4662-99c1-0527bacddc23","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":8}}');
INSERT INTO requests VALUES('c90cab28-2a65-4934-88e7-1dce320c0e2c','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"c90cab28-2a65-4934-88e7-1dce320c0e2c","team":"00000000-0000-4000-8000-00000000beef","expected_revision":8,"op":"observe_task","node":1,"observation":{"revision":"token-2","previous_revision":"token-1","stage":"in_progress","cancelled":false,"rank":"a00000001","title":"Task 1","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":9}}');
INSERT INTO requests VALUES('7407e333-171d-43f7-b31c-a7ebfc003694','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"7407e333-171d-43f7-b31c-a7ebfc003694","team":"00000000-0000-4000-8000-00000000beef","expected_revision":9,"op":"observe_task","node":1,"observation":{"revision":"token-3","previous_revision":"token-2","stage":"in_review","cancelled":false,"rank":"a00000001","title":"Task 1","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":10}}');
INSERT INTO requests VALUES('1ad54dc2-84f0-4da9-bc1e-9d38b555f343','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"1ad54dc2-84f0-4da9-bc1e-9d38b555f343","team":"00000000-0000-4000-8000-00000000beef","expected_revision":10,"op":"record_report","node":1,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":11,"item":1}}');
INSERT INTO requests VALUES('6f93c2ee-278e-408f-abfc-ce9bba5c8ace','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"6f93c2ee-278e-408f-abfc-ce9bba5c8ace","team":"00000000-0000-4000-8000-00000000beef","expected_revision":11,"op":"cancel","node":1,"reason":"Cancel for the legacy hold"}','{"Success":{"ok":true,"revision":12}}');
INSERT INTO requests VALUES('dd5f3af6-be88-4195-823f-24c502cf1b7d','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"dd5f3af6-be88-4195-823f-24c502cf1b7d","team":"00000000-0000-4000-8000-00000000beef","expected_revision":12,"op":"record_cancel","node":1,"decision":1,"state":"refused","reason":"Still running","attempt":0,"attempt_id":""}','{"Success":{"ok":true,"revision":13,"node":1,"item":2}}');
INSERT INTO requests VALUES('567bd974-7ed8-41f3-be44-e06b67f5420c','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"567bd974-7ed8-41f3-be44-e06b67f5420c","team":"00000000-0000-4000-8000-00000000beef","expected_revision":13,"op":"release","node":1}','{"Success":{"ok":true,"revision":14}}');
INSERT INTO requests VALUES('09a941e5-64c3-4381-81bb-77fb707dfabb','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"09a941e5-64c3-4381-81bb-77fb707dfabb","team":"00000000-0000-4000-8000-00000000beef","expected_revision":14,"op":"hold","node":1}','{"Success":{"ok":true,"revision":15}}');
INSERT INTO requests VALUES('35e782b8-3299-49bf-80b6-4fd732f7ed67','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"35e782b8-3299-49bf-80b6-4fd732f7ed67","team":"00000000-0000-4000-8000-00000000beef","expected_revision":15,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000002"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":16,"node":2}}');
INSERT INTO requests VALUES('e7e7eeb5-1083-4c91-9e8a-8b5fe8ed6ea6','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"e7e7eeb5-1083-4c91-9e8a-8b5fe8ed6ea6","team":"00000000-0000-4000-8000-00000000beef","expected_revision":16,"op":"observe_task","node":2,"observation":{"revision":"token-4","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000002","title":"Task 2","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":17}}');
INSERT INTO requests VALUES('cfc51ecc-00a7-4725-809a-5aaa4059f21e','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"cfc51ecc-00a7-4725-809a-5aaa4059f21e","team":"00000000-0000-4000-8000-00000000beef","expected_revision":17,"op":"handout_intent","node":2,"member":"m1","effect_request_id":"8a7dc696-55e9-4104-84e8-7d74f8db8029"}','{"Success":{"ok":true,"revision":18,"node":2,"item":2}}');
INSERT INTO requests VALUES('00ad0cd0-40c5-49af-b38b-937b16dff5cc','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"00ad0cd0-40c5-49af-b38b-937b16dff5cc","team":"00000000-0000-4000-8000-00000000beef","expected_revision":18,"op":"handout_outcome","effect_request_id":"8a7dc696-55e9-4104-84e8-7d74f8db8029","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":19}}');
INSERT INTO requests VALUES('66e3ce64-4e80-4000-934e-33972a88b205','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"66e3ce64-4e80-4000-934e-33972a88b205","team":"00000000-0000-4000-8000-00000000beef","expected_revision":19,"op":"observe_task","node":2,"observation":{"revision":"token-5","previous_revision":"token-4","stage":"in_progress","cancelled":false,"rank":"a00000002","title":"Task 2","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":20}}');
INSERT INTO requests VALUES('02360aee-8167-4ac2-90eb-6d25a7648ab4','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"02360aee-8167-4ac2-90eb-6d25a7648ab4","team":"00000000-0000-4000-8000-00000000beef","expected_revision":20,"op":"observe_task","node":2,"observation":{"revision":"token-6","previous_revision":"token-5","stage":"in_review","cancelled":false,"rank":"a00000002","title":"Task 2","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":21}}');
INSERT INTO requests VALUES('01bcabda-ad03-42d7-894b-911f1461e914','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"01bcabda-ad03-42d7-894b-911f1461e914","team":"00000000-0000-4000-8000-00000000beef","expected_revision":21,"op":"record_report","node":2,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":22,"item":3}}');
INSERT INTO requests VALUES('655a1185-a31d-4c43-8f59-7a361b6758a7','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"655a1185-a31d-4c43-8f59-7a361b6758a7","team":"00000000-0000-4000-8000-00000000beef","expected_revision":22,"op":"hold","node":2}','{"Success":{"ok":true,"revision":23}}');
INSERT INTO requests VALUES('c0b069fc-ca59-49c2-98d3-f18fe2e7eb4a','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"c0b069fc-ca59-49c2-98d3-f18fe2e7eb4a","team":"00000000-0000-4000-8000-00000000beef","expected_revision":23,"op":"cancel","node":2,"reason":"Cancel for the legacy hold"}','{"Success":{"ok":true,"revision":24}}');
INSERT INTO requests VALUES('b83b5994-1066-449e-acb5-3feebcb4494c','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"b83b5994-1066-449e-acb5-3feebcb4494c","team":"00000000-0000-4000-8000-00000000beef","expected_revision":24,"op":"record_cancel","node":2,"decision":2,"state":"refused","reason":"Still running","attempt":0,"attempt_id":""}','{"Success":{"ok":true,"revision":25,"node":2,"item":4}}');
INSERT INTO requests VALUES('284b6868-f7ea-4279-9279-331108398c46','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"284b6868-f7ea-4279-9279-331108398c46","team":"00000000-0000-4000-8000-00000000beef","expected_revision":25,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000003"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":26,"node":3}}');
INSERT INTO requests VALUES('1ffa3adf-ee26-4dda-8a59-2c4a9912c4d9','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"1ffa3adf-ee26-4dda-8a59-2c4a9912c4d9","team":"00000000-0000-4000-8000-00000000beef","expected_revision":26,"op":"observe_task","node":3,"observation":{"revision":"token-7","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000003","title":"Task 3","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":27}}');
INSERT INTO requests VALUES('454d5ff9-d02b-40d5-bc86-2564e881068d','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"454d5ff9-d02b-40d5-bc86-2564e881068d","team":"00000000-0000-4000-8000-00000000beef","expected_revision":27,"op":"hold","node":3}','{"Success":{"ok":true,"revision":28}}');
INSERT INTO requests VALUES('3dd0f36c-f087-4e7b-9b65-25eb0d577019','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"3dd0f36c-f087-4e7b-9b65-25eb0d577019","team":"00000000-0000-4000-8000-00000000beef","expected_revision":28,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000004"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":29,"node":4}}');
INSERT INTO requests VALUES('ae407eb8-07c6-49c8-bb01-7e10b87f450f','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"ae407eb8-07c6-49c8-bb01-7e10b87f450f","team":"00000000-0000-4000-8000-00000000beef","expected_revision":29,"op":"observe_task","node":4,"observation":{"revision":"token-8","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000004","title":"Task 4","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":30}}');
INSERT INTO requests VALUES('0e3bf13a-855a-457d-9795-54c73e3bfbb4','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"0e3bf13a-855a-457d-9795-54c73e3bfbb4","team":"00000000-0000-4000-8000-00000000beef","expected_revision":30,"op":"handout_intent","node":4,"member":"m1","effect_request_id":"dc2dd42c-e015-4373-a7d3-229df2bc2ab9"}','{"Success":{"ok":true,"revision":31,"node":4,"item":3}}');
INSERT INTO requests VALUES('651e7c51-3aad-460f-8a5d-d67d0f4c6afc','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"651e7c51-3aad-460f-8a5d-d67d0f4c6afc","team":"00000000-0000-4000-8000-00000000beef","expected_revision":31,"op":"handout_outcome","effect_request_id":"dc2dd42c-e015-4373-a7d3-229df2bc2ab9","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":32}}');
INSERT INTO requests VALUES('cfde9455-2a31-4941-b6ad-fe3bc2883d59','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"cfde9455-2a31-4941-b6ad-fe3bc2883d59","team":"00000000-0000-4000-8000-00000000beef","expected_revision":32,"op":"observe_task","node":4,"observation":{"revision":"token-9","previous_revision":"token-8","stage":"in_progress","cancelled":false,"rank":"a00000004","title":"Task 4","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":33}}');
INSERT INTO requests VALUES('02cf0c9e-e88e-4103-93de-0bcdb72fe033','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"02cf0c9e-e88e-4103-93de-0bcdb72fe033","team":"00000000-0000-4000-8000-00000000beef","expected_revision":33,"op":"observe_task","node":4,"observation":{"revision":"token-10","previous_revision":"token-9","stage":"in_review","cancelled":false,"rank":"a00000004","title":"Task 4","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":34}}');
INSERT INTO requests VALUES('ae50aff6-4383-4f99-b351-c1b1b5369302','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"ae50aff6-4383-4f99-b351-c1b1b5369302","team":"00000000-0000-4000-8000-00000000beef","expected_revision":34,"op":"record_report","node":4,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":35,"item":5}}');
INSERT INTO requests VALUES('156b13f8-d31a-47f7-b0a0-4f3a67e1fbb2','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"156b13f8-d31a-47f7-b0a0-4f3a67e1fbb2","team":"00000000-0000-4000-8000-00000000beef","expected_revision":35,"op":"cancel","node":4,"reason":"Cancel for the legacy hold"}','{"Success":{"ok":true,"revision":36}}');
INSERT INTO requests VALUES('af675923-0343-412e-88fd-4c93b32a58fa','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"af675923-0343-412e-88fd-4c93b32a58fa","team":"00000000-0000-4000-8000-00000000beef","expected_revision":36,"op":"record_cancel","node":4,"decision":3,"state":"refused","reason":"Still running","attempt":0,"attempt_id":""}','{"Success":{"ok":true,"revision":37,"node":4,"item":6}}');
INSERT INTO requests VALUES('abc68d63-a1f0-407a-a51f-36dddf87aea1','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"abc68d63-a1f0-407a-a51f-36dddf87aea1","team":"00000000-0000-4000-8000-00000000beef","expected_revision":37,"op":"release","node":4}','{"Success":{"ok":true,"revision":38}}');
INSERT INTO requests VALUES('ebfa01b8-d95e-4d36-bf07-e2e135cfb06a','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"ebfa01b8-d95e-4d36-bf07-e2e135cfb06a","team":"00000000-0000-4000-8000-00000000beef","expected_revision":38,"op":"hold","node":4}','{"Success":{"ok":true,"revision":39}}');
INSERT INTO requests VALUES('771a9cf6-e3f5-4812-bcea-9ec809d21612','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"771a9cf6-e3f5-4812-bcea-9ec809d21612","team":"00000000-0000-4000-8000-00000000beef","expected_revision":39,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000005"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":40,"node":5}}');
INSERT INTO requests VALUES('a518fc37-d2d3-4d4f-b7e9-863483d2cc3a','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"a518fc37-d2d3-4d4f-b7e9-863483d2cc3a","team":"00000000-0000-4000-8000-00000000beef","expected_revision":40,"op":"observe_task","node":5,"observation":{"revision":"token-11","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000005","title":"Task 5","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":41}}');
INSERT INTO requests VALUES('cbb0a854-0891-4425-94d2-fe8b478ccac2','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"cbb0a854-0891-4425-94d2-fe8b478ccac2","team":"00000000-0000-4000-8000-00000000beef","expected_revision":41,"op":"handout_intent","node":5,"member":"m1","effect_request_id":"0e0b03c6-52f3-4d2c-9da9-d0b222a075e3"}','{"Success":{"ok":true,"revision":42,"node":5,"item":4}}');
INSERT INTO requests VALUES('9b915b70-97da-487f-88a0-a0200b3c038c','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"9b915b70-97da-487f-88a0-a0200b3c038c","team":"00000000-0000-4000-8000-00000000beef","expected_revision":42,"op":"handout_outcome","effect_request_id":"0e0b03c6-52f3-4d2c-9da9-d0b222a075e3","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":43}}');
INSERT INTO requests VALUES('42885594-1ea5-49f5-9d1b-a6e0526e3299','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"42885594-1ea5-49f5-9d1b-a6e0526e3299","team":"00000000-0000-4000-8000-00000000beef","expected_revision":43,"op":"observe_task","node":5,"observation":{"revision":"token-12","previous_revision":"token-11","stage":"in_progress","cancelled":false,"rank":"a00000005","title":"Task 5","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":44}}');
INSERT INTO requests VALUES('60627a86-35b1-43e3-9946-f3e9d1da33fd','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"60627a86-35b1-43e3-9946-f3e9d1da33fd","team":"00000000-0000-4000-8000-00000000beef","expected_revision":44,"op":"observe_task","node":5,"observation":{"revision":"token-13","previous_revision":"token-12","stage":"in_review","cancelled":false,"rank":"a00000005","title":"Task 5","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":45}}');
INSERT INTO requests VALUES('68bab8c4-41ed-47d4-a37e-27c71cba8fd6','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"68bab8c4-41ed-47d4-a37e-27c71cba8fd6","team":"00000000-0000-4000-8000-00000000beef","expected_revision":45,"op":"record_report","node":5,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":46,"item":7}}');
INSERT INTO requests VALUES('1754e14a-6642-408c-a8de-3733c692b6b3','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"1754e14a-6642-408c-a8de-3733c692b6b3","team":"00000000-0000-4000-8000-00000000beef","expected_revision":46,"op":"return","node":5,"reason":"Reopen for the legacy hold"}','{"Success":{"ok":true,"revision":47}}');
INSERT INTO requests VALUES('37292594-721a-479a-9b4a-311f365ce3c0','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"37292594-721a-479a-9b4a-311f365ce3c0","team":"00000000-0000-4000-8000-00000000beef","expected_revision":47,"op":"reopen_intent","node":5,"member":"m1","effect_request_id":"35f3950a-819f-4392-9349-a9264908e713"}','{"Success":{"ok":true,"revision":48,"node":5,"item":5}}');
INSERT INTO requests VALUES('47402f97-e59e-45d4-8f16-3f1fb60f4d45','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"47402f97-e59e-45d4-8f16-3f1fb60f4d45","team":"00000000-0000-4000-8000-00000000beef","expected_revision":48,"op":"reopen_outcome","effect_request_id":"35f3950a-819f-4392-9349-a9264908e713","outcome":{"status":"refused","reason":"Not delivered"}}','{"Success":{"ok":true,"revision":49}}');
INSERT INTO requests VALUES('9f4e90ea-7c87-447a-8cac-d6b17483928d','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"9f4e90ea-7c87-447a-8cac-d6b17483928d","team":"00000000-0000-4000-8000-00000000beef","expected_revision":49,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000006"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":50,"node":6}}');
INSERT INTO requests VALUES('7a278d23-6bfe-41b3-8f42-bcb4892d226b','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"7a278d23-6bfe-41b3-8f42-bcb4892d226b","team":"00000000-0000-4000-8000-00000000beef","expected_revision":50,"op":"observe_task","node":6,"observation":{"revision":"token-14","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000006","title":"Task 6","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":51}}');
INSERT INTO requests VALUES('932ce2f6-ccaf-49c2-a48a-02ca4af6bb94','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"932ce2f6-ccaf-49c2-a48a-02ca4af6bb94","team":"00000000-0000-4000-8000-00000000beef","expected_revision":51,"op":"handout_intent","node":6,"member":"m1","effect_request_id":"7229a188-f5ad-40c2-837e-1ffd62b986dd"}','{"Success":{"ok":true,"revision":52,"node":6,"item":6}}');
INSERT INTO requests VALUES('585147f4-1f89-41ea-abbd-7423715b37dc','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"585147f4-1f89-41ea-abbd-7423715b37dc","team":"00000000-0000-4000-8000-00000000beef","expected_revision":52,"op":"handout_outcome","effect_request_id":"7229a188-f5ad-40c2-837e-1ffd62b986dd","outcome":{"status":"submitted","receipt":"receipt-example"}}','{"Success":{"ok":true,"revision":53}}');
INSERT INTO requests VALUES('61cd97c4-e172-4983-be37-adc9c93550c8','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"61cd97c4-e172-4983-be37-adc9c93550c8","team":"00000000-0000-4000-8000-00000000beef","expected_revision":53,"op":"observe_task","node":6,"observation":{"revision":"token-15","previous_revision":"token-14","stage":"in_progress","cancelled":false,"rank":"a00000006","title":"Task 6","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":54}}');
INSERT INTO requests VALUES('abe866c7-52bd-4420-93e1-1af9328ed67d','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"abe866c7-52bd-4420-93e1-1af9328ed67d","team":"00000000-0000-4000-8000-00000000beef","expected_revision":54,"op":"observe_task","node":6,"observation":{"revision":"token-16","previous_revision":"token-15","stage":"in_review","cancelled":false,"rank":"a00000006","title":"Task 6","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":55}}');
INSERT INTO requests VALUES('cbe9a249-7f84-4ca1-a74a-4fee5eb06e27','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"cbe9a249-7f84-4ca1-a74a-4fee5eb06e27","team":"00000000-0000-4000-8000-00000000beef","expected_revision":55,"op":"record_report","node":6,"generation":1,"member":"m1","text":"Implemented and checked","commit":"0123456789abcdef0123456789abcdef01234567"}','{"Success":{"ok":true,"revision":56,"item":8}}');
INSERT INTO requests VALUES('a81024b2-5441-488d-b1c9-10ea15bc5845','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"a81024b2-5441-488d-b1c9-10ea15bc5845","team":"00000000-0000-4000-8000-00000000beef","expected_revision":56,"op":"return","node":6,"reason":"Reopen for the legacy hold"}','{"Success":{"ok":true,"revision":57}}');
INSERT INTO requests VALUES('5c3b5660-8855-4730-9dfb-f0b1895d5fe3','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"5c3b5660-8855-4730-9dfb-f0b1895d5fe3","team":"00000000-0000-4000-8000-00000000beef","expected_revision":57,"op":"reopen_intent","node":6,"member":"m1","effect_request_id":"05481723-db0e-47d5-8eb0-63ba5cf0acd2"}','{"Success":{"ok":true,"revision":58,"node":6,"item":7}}');
INSERT INTO requests VALUES('f93527a5-0e76-4fd7-a528-f845ef280553','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"f93527a5-0e76-4fd7-a528-f845ef280553","team":"00000000-0000-4000-8000-00000000beef","expected_revision":58,"op":"reopen_outcome","effect_request_id":"05481723-db0e-47d5-8eb0-63ba5cf0acd2","outcome":{"status":"cutting_over","reason":"Replacement starting"}}','{"Success":{"ok":true,"revision":59,"item":9}}');
INSERT INTO requests VALUES('2daf0e97-7104-4193-9bb3-4f1725442176','00000000-0000-4000-8000-00000000beef','{"kind":"session","person":"local","team":"00000000-0000-4000-8000-00000000beef","member":"lead","session":{"machine":"00000000-0000-4000-8000-0000000000aa","session_id":"launch_id:00000000-0000-4000-8000-000000000064"}}','{"request_id":"2daf0e97-7104-4193-9bb3-4f1725442176","team":"00000000-0000-4000-8000-00000000beef","expected_revision":59,"op":"release","node":6}','{"Success":{"ok":true,"revision":60}}');
INSERT INTO requests VALUES('a7cf26d4-468a-4aa6-8e55-735cbfa1f144','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"a7cf26d4-468a-4aa6-8e55-735cbfa1f144","team":"00000000-0000-4000-8000-00000000beef","expected_revision":60,"op":"hold","node":6}','{"Success":{"ok":true,"revision":61}}');
INSERT INTO requests VALUES('0d62f863-1075-40dc-bb87-4dc0cd509264','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"0d62f863-1075-40dc-bb87-4dc0cd509264","team":"00000000-0000-4000-8000-00000000beef","expected_revision":61,"op":"add_task","task":{"project":"example-project","uuid":"00000000-0000-4000-8000-000000000007"},"role":null,"pipeline":null}','{"Success":{"ok":true,"revision":62,"node":7}}');
INSERT INTO requests VALUES('8a95884b-e74b-465e-a359-e2a35871d536','00000000-0000-4000-8000-00000000beef','{"kind":"service","person":"local","team":"00000000-0000-4000-8000-00000000beef"}','{"request_id":"8a95884b-e74b-465e-a359-e2a35871d536","team":"00000000-0000-4000-8000-00000000beef","expected_revision":62,"op":"observe_task","node":7,"observation":{"revision":"token-17","previous_revision":"","stage":"todo","cancelled":false,"rank":"a00000007","title":"Task 7","attempt":0,"attempt_id":"","attempt_conflict":false}}','{"Success":{"ok":true,"revision":63}}');
INSERT INTO requests VALUES('dafe8a2c-51de-4118-bc4d-71f7ea4c8764','00000000-0000-4000-8000-00000000beef','{"kind":"person","person":"local","origin":"desktop_ui"}','{"request_id":"dafe8a2c-51de-4118-bc4d-71f7ea4c8764","team":"00000000-0000-4000-8000-00000000beef","expected_revision":63,"op":"hold","node":7}','{"Success":{"ok":true,"revision":64}}');
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
INSERT INTO timers_extension VALUES(1,9);
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
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','bbb56746-dbd9-4662-99c1-0527bacddc23',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','8a7dc696-55e9-4104-84e8-7d74f8db8029',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','dc2dd42c-e015-4373-a7d3-229df2bc2ab9',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','0e0b03c6-52f3-4d2c-9da9-d0b222a075e3',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','35f3950a-819f-4392-9349-a9264908e713',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','7229a188-f5ad-40c2-837e-1ffd62b986dd',1234);
INSERT INTO effect_outcomes VALUES('00000000-0000-4000-8000-00000000beef','05481723-db0e-47d5-8eb0-63ba5cf0acd2',1234);
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
