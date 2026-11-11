//! Additive Teams facts using the strict observer's pinned, bounded reads.
//! A label is only a route: every final record must carry the launch UUID.
use super::unix::*;
use super::{TeamObservation, TeamRecord};
use std::{path::Path, time::Instant};
struct Reads<'a> {
    counts: &'a mut Counts,
    bytes: ByteBudget,
    files: Vec<(Root, String, Option<Image>)>,
    absent: Vec<PathBuf>,
}
impl<'a> Reads<'a> {
    fn new(counts: &'a mut Counts) -> Self {
        Self {
            counts,
            bytes: ByteBudget::new(),
            files: vec![],
            absent: vec![],
        }
    }
    fn read(&mut self, path: &Path, cap: usize) -> Checked<Option<Value>> {
        valid_path(path)?;
        let parent = path.parent().ok_or(Fault)?;
        if path_stamp(parent, self.counts)?.is_none() {
            self.absent.push(parent.into());
            return Ok(None);
        }
        let root = Root::pin(parent, self.counts)?;
        let leaf = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(Fault)?
            .to_owned();
        let image = capture(&root, &leaf, cap, &mut self.bytes, self.counts)?;
        let value = image.as_ref().map(json).transpose()?;
        self.files.push((root, leaf, image));
        Ok(value)
    }
    fn finish(&mut self) -> Checked<()> {
        let mut bytes = ByteBudget::new();
        for (root, leaf, image) in &self.files {
            recheck(root, leaf, image.as_ref(), &mut bytes, self.counts)?;
            root.finish(self.counts)?;
        }
        for path in &self.absent {
            if path_stamp(path, self.counts)?.is_some() {
                return Err(Fault);
            }
        }
        Ok(())
    }
    fn label(&mut self, state: &Path, label: &str) -> Checked<Option<Value>> {
        if !name(label) {
            return Err(Fault);
        }
        let leaf = format!("{:x}.json", Sha256::digest(label.as_bytes()));
        if let Some(agent @ ("dsh" | "opencode")) = label.split('/').next() {
            return self.read(&state.join(agent).join("bindings").join(&leaf), RAW);
        }
        let route = self.read(&state.join("name_routes").join(&leaf), SMALL)?;
        let path = if route.is_some() {
            state.join("bindings").join(&leaf)
        } else {
            state.join(&leaf)
        };
        self.read(&path, RAW)
    }
    fn by_id(&mut self, state: &Path, id: &str, agent: &str) -> Checked<Option<Value>> {
        if !canonical(id) {
            return Err(Fault);
        }
        let folder = if matches!(agent, "dsh" | "opencode") {
            state.join(agent).join("sessions")
        } else {
            state.join("sessions")
        };
        if let Some(v) = self.read(&folder.join(format!("{id}.json")), RAW)? {
            if v["session_id"] != id || v["layout"] != "id" {
                return Err(Fault);
            }
            return Ok(Some(v));
        }
        let Some(index) = self.read(&state.join("ids").join(format!("{id}.json")), SMALL)? else {
            return Ok(None);
        };
        let value = self.label(state, text(&index, "name")?)?;
        if value.as_ref().is_some_and(|v| v["session_id"] != id) {
            return Err(Fault);
        }
        Ok(value)
    }
}
fn record(value: Value, launch: &str) -> Checked<TeamObservation> {
    let actual = text(&value, "launch_id")?;
    if actual != launch {
        return Ok(TeamObservation::Mismatch {
            field: "launch_id".into(),
        });
    }
    if value["rename_shadow"] == true {
        return Err(Fault);
    }
    let get = |key: &str, max: usize| -> Checked<String> {
        let s = match value.get(key) {
            None | Some(Value::Null) => "",
            Some(Value::String(s)) => s,
            _ => return Err(Fault),
        };
        if s.len() > max || s.chars().any(char::is_control) {
            return Err(Fault);
        }
        Ok(s.into())
    };
    let label = get("name", 512)?;
    if !name(&label) {
        return Err(Fault);
    }
    let agent = get("agent", 32)?;
    if !matches!(
        agent.as_str(),
        "claude" | "codex" | "kimi" | "dsh" | "opencode"
    ) {
        return Err(Fault);
    }
    let account = get("account_id", 256)?;
    Ok(TeamObservation::Record(TeamRecord {
        launch_id: actual.into(),
        name: label,
        agent: agent.clone(),
        session_id: get("session_id", 256)?,
        run_id: get("run_id", 256)?,
        conversation_id: get("conversation_id", 2048)?,
        cwd: get("cwd", 4096)?,
        account: if account.is_empty() {
            format!("native-{agent}")
        } else {
            account
        },
    }))
}
fn selected_team(state: &Path, launch: &str, r: &mut Reads<'_>) -> Checked<TeamObservation> {
    let start = r.read(
        &state.join("launches").join(format!("{launch}.json")),
        SMALL,
    )?;
    let end = r.read(
        &state.join("launches").join(format!("{launch}.end.json")),
        SMALL,
    )?;
    if let Some(end) = end {
        if end["launch_id"] != launch {
            return Ok(TeamObservation::Mismatch {
                field: "launch_id".into(),
            });
        }
        let code = end["code"]
            .as_i64()
            .and_then(|c| i32::try_from(c).ok())
            .ok_or(Fault)?;
        return Ok(TeamObservation::Ended { code });
    }
    let Some(start) = start else {
        return Ok(TeamObservation::Absent { legacy: true });
    };
    if start["launch_id"] != launch {
        return Ok(TeamObservation::Mismatch {
            field: "launch_id".into(),
        });
    }
    let label = text(&start, "name")?;
    if !name(label) {
        return Err(Fault);
    }
    let agent = label.split('/').next().ok_or(Fault)?;
    if let Some(id) = start["session_id"].as_str() {
        if let Some(v) = r.by_id(state, id, agent)? {
            return record(v, launch);
        }
    }
    let mut candidate = r.label(state, label)?;
    for _ in 0..3 {
        let Some(v) = &candidate else { break };
        if v["launch_id"] != launch || v["rename_shadow"] != true {
            return record(v.clone(), launch);
        }
        let target = text(v, "rename_target")?.to_owned();
        candidate = r.label(state, &target)?;
    }
    if candidate.is_some() {
        return Err(Fault);
    }
    let leaf = format!("{:x}.json", Sha256::digest(label.as_bytes()));
    if let Some(index) = r.read(&state.join("labels").join(leaf), SMALL)? {
        if index["label"] != label {
            return Err(Fault);
        }
        let ids = index["ids"]
            .as_array()
            .filter(|ids| ids.len() <= 8)
            .ok_or(Fault)?;
        let mut found = None;
        for id in ids {
            if let Some(v) = r.by_id(state, id.as_str().ok_or(Fault)?, agent)? {
                if v["launch_id"] == launch {
                    if found.is_some() {
                        return Err(Fault);
                    }
                    found = Some(v);
                }
            }
        }
        if let Some(v) = found {
            return record(v, launch);
        }
    }
    Ok(TeamObservation::Absent {
        legacy: start["session_id"].is_null(),
    })
}
pub(super) fn observe(state: &Path, launch: &str, deadline: Instant) -> TeamObservation {
    if !canonical(launch) {
        return TeamObservation::Unavailable;
    }
    let mut counts = Counts {
        deadline: Some(deadline),
        ..Counts::default()
    };
    let mut r = Reads::new(&mut counts);
    let result =
        selected_team(state, launch, &mut r).and_then(|result| r.finish().map(|()| result));
    if Instant::now() >= deadline {
        return TeamObservation::Unavailable;
    }
    result.unwrap_or(TeamObservation::Unavailable)
}

#[cfg(test)]
mod tests;
