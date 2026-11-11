CREATE INDEX IF NOT EXISTS teams_live ON teams(id)
 WHERE status IN ('"running"','"paused"');
CREATE INDEX IF NOT EXISTS cards_waiting ON cards(seq) WHERE state='open';
CREATE INDEX IF NOT EXISTS effects_report ON effects(team,node,generation,seq DESC)
 WHERE state NOT IN ('refused','superseded');
