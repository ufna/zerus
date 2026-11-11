ALTER TABLE teams ADD COLUMN person_paused INTEGER NOT NULL DEFAULT 0;
UPDATE teams SET person_paused=1 WHERE status='"paused"';
ALTER TABLE effects ADD COLUMN kind TEXT NOT NULL DEFAULT 'handout';
ALTER TABLE cards ADD COLUMN decision_seq INTEGER;
ALTER TABLE decisions ADD COLUMN state TEXT NOT NULL DEFAULT 'settled';
CREATE TABLE node_identity_v2 (team TEXT NOT NULL REFERENCES teams(id), id INTEGER NOT NULL,
 task_project TEXT NOT NULL, task_uuid TEXT NOT NULL, PRIMARY KEY(team,id));
INSERT INTO node_identity_v2 SELECT * FROM node_identity;
DROP TABLE node_identity;
ALTER TABLE node_identity_v2 RENAME TO node_identity;
CREATE INDEX identity_task ON node_identity(task_project,task_uuid,team,id);
CREATE TABLE live_tasks (task_project TEXT NOT NULL, task_uuid TEXT NOT NULL, team TEXT NOT NULL, node INTEGER NOT NULL,
 PRIMARY KEY(task_project,task_uuid), FOREIGN KEY(team,node) REFERENCES node_identity(team,id));
CREATE INDEX live_tasks_team ON live_tasks(team,node);
INSERT INTO live_tasks SELECT i.task_project,i.task_uuid,i.team,i.id FROM node_identity i
 JOIN teams t ON t.id=i.team WHERE t.status NOT IN ('"stopped"','"archived"')
 AND NOT EXISTS(SELECT 1 FROM node_archive a WHERE a.team=i.team AND a.id=i.id AND a.task_cancelled=1);
CREATE TABLE suggestions (team TEXT NOT NULL, node INTEGER NOT NULL, role TEXT, pipeline TEXT, PRIMARY KEY(team,node),
 FOREIGN KEY(team,node) REFERENCES node_identity(team,id));
DROP INDEX nodes_state;
DROP INDEX archive_state;
ALTER TABLE nodes DROP COLUMN node_state;
ALTER TABLE node_archive DROP COLUMN node_state;
ALTER TABLE nodes ADD COLUMN integration_required INTEGER NOT NULL DEFAULT 1;
ALTER TABLE node_archive ADD COLUMN integration_required INTEGER NOT NULL DEFAULT 1;
UPDATE nodes SET integration_required=0 WHERE team IN (SELECT id FROM teams WHERE json_extract(settings,'$.integration')='none');
UPDATE node_archive SET integration_required=0 WHERE team IN (SELECT id FROM teams WHERE json_extract(settings,'$.integration')='none');
ALTER TABLE nodes ADD COLUMN node_state TEXT GENERATED ALWAYS AS (CASE
   WHEN task_cancelled THEN 'cancelled'
   WHEN task_stage='done' AND integration_required AND COALESCE(json_extract(data,'$.kind'),'task')<>'stage' AND NOT integrated THEN 'reported'
   WHEN task_stage='done' THEN 'accepted'
   WHEN task_stage='unobserved' THEN 'waiting'
   WHEN NOT admitted OR NOT routed OR task_stage='backlog' THEN 'waiting'
   WHEN task_stage='in_progress' THEN 'in_progress'
   WHEN task_stage='in_review' THEN 'reported' ELSE 'pending' END) VIRTUAL;
ALTER TABLE node_archive ADD COLUMN node_state TEXT GENERATED ALWAYS AS (CASE
   WHEN task_cancelled THEN 'cancelled'
   WHEN task_stage='done' AND integration_required AND COALESCE(json_extract(data,'$.kind'),'task')<>'stage' AND NOT integrated THEN 'reported'
   WHEN task_stage='done' THEN 'accepted'
   WHEN task_stage='unobserved' THEN 'waiting'
   WHEN NOT admitted OR NOT routed OR task_stage='backlog' THEN 'waiting'
   WHEN task_stage='in_progress' THEN 'in_progress'
   WHEN task_stage='in_review' THEN 'reported' ELSE 'pending' END) VIRTUAL;
CREATE INDEX nodes_state ON nodes(team,node_state,id);
CREATE INDEX archive_state ON node_archive(team,node_state,id);
UPDATE decisions SET state=CASE WHEN kind='accept' THEN 'ready' ELSE 'pending' END
 WHERE seq IN (SELECT MAX(d.seq) FROM decisions d JOIN nodes n ON n.team=d.team AND n.id=d.node
 WHERE n.requested IS NOT NULL GROUP BY d.team,d.node);
CREATE UNIQUE INDEX decision_pending ON decisions(team,node) WHERE state<>'settled';
PRAGMA user_version=2;
