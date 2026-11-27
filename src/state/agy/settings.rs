//! ACP settings are observations of this worker's loaded conversation. A new
//! worker starts with no settings evidence, even when it resumes the same ID.
use super::*;
use serde::Serialize;
use std::collections::BTreeSet;

const MAX_OPTIONS: usize = 64;
const MAX_LABEL: usize = 256;
const MAX_ID: usize = 128;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct Choice {
    id: String,
    label: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct Select {
    id: String,
    current: String,
    choices: Vec<Choice>,
}

impl Select {
    fn current(&self) -> Option<&Choice> {
        self.choices.iter().find(|choice| choice.id == self.current)
    }

    fn offers(&self, id: &str) -> bool {
        self.choices.iter().any(|choice| choice.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct Options {
    model: Select,
    effort: Option<Select>,
    mode: Option<Select>,
    legacy: bool,
}

#[derive(Deserialize, Serialize)]
struct Snapshot {
    revision: String,
    options: Options,
}

fn id(value: &Value) -> Option<&str> {
    value.as_str().filter(|id| valid_id(id))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID && !id.chars().any(char::is_control)
}

fn choice(value: &Value, key: &str) -> Option<Choice> {
    let id = id(&value[key])?;
    let label = value["name"].as_str().filter(|label| {
        !label.is_empty() && label.len() <= MAX_LABEL && !label.chars().any(char::is_control)
    })?;
    Some(Choice {
        id: id.into(),
        label: label.into(),
    })
}

fn choose(control: &Value) -> Option<Select> {
    if control["type"] != "select" {
        return None;
    }
    let options = control["options"].as_array()?;
    if options.len() > MAX_OPTIONS {
        return None;
    }
    let grouped = options
        .first()
        .is_some_and(|item| item.get("group").is_some());
    let mut choices = Vec::new();
    for item in options {
        if grouped {
            let group = item["options"].as_array()?;
            if id(&item["group"]).is_none() || group.len() > MAX_OPTIONS - choices.len() {
                return None;
            }
            for item in group {
                choices.push(choice(item, "value")?);
            }
        } else {
            choices.push(choice(item, "value")?);
        }
    }
    selected(id(&control["id"])?, id(&control["currentValue"])?, choices)
}

fn selected(control: &str, current: &str, choices: Vec<Choice>) -> Option<Select> {
    let ids: BTreeSet<_> = choices.iter().map(|choice| choice.id.as_str()).collect();
    (ids.len() == choices.len() && ids.contains(current)).then(|| Select {
        id: control.into(),
        current: current.into(),
        choices,
    })
}

fn options(value: &Value) -> Option<Options> {
    if let Some(native) = value.get("configOptions") {
        let native = native.as_array()?;
        if native.len() > MAX_OPTIONS {
            return None;
        }
        let find = |category, expected_id| {
            let mut matching = native
                .iter()
                .filter(|option| option["category"] == category);
            let control = matching.next()?;
            if control["id"] != expected_id
                || matching.next().is_some()
                || native
                    .iter()
                    .filter(|other| other["id"] == control["id"])
                    .count()
                    != 1
            {
                return None;
            }
            choose(control)
        };
        return Some(Options {
            // Antigravity's verified model controls. ACP categories alone
            // do not authorize a write to an arbitrary native setting.
            model: find("model", "model")?,
            effort: find("thought_level", "thought_level"),
            mode: find("mode", "mode"),
            legacy: false,
        });
    }

    let legacy = &value["models"];
    let rows = legacy["availableModels"].as_array()?;
    if rows.len() > MAX_OPTIONS {
        return None;
    }
    let choices = rows
        .iter()
        .map(|row| choice(row, "modelId"))
        .collect::<Option<Vec<_>>>()?;
    Some(Options {
        model: selected("", id(&legacy["currentModelId"])?, choices)?,
        effort: None,
        mode: None,
        legacy: true,
    })
}

fn snapshot(record: &Value) -> Option<Snapshot> {
    if !same_scope(record) {
        return None;
    }
    serde_json::from_value(record["agy_config"].clone()).ok()
}

pub(super) fn bind_scope(record: &mut Value) {
    record["agy_config_scope"] = json!({
        "worker":record["agy_worker"],"run_id":record["run_id"],
        "conversation_id":record["conversation_id"],"session_id":record["session_id"]
    });
}

pub(super) fn same_scope(record: &Value) -> bool {
    let scope = &record["agy_config_scope"];
    scope.is_object()
        && !record["agy_worker"].is_null()
        && scope["worker"] == record["agy_worker"]
        && ["run_id", "conversation_id", "session_id"]
            .into_iter()
            .all(|key| scope[key] == record[key])
}

pub(super) fn observe(record: &mut Value, value: &Value) {
    let previous = snapshot(record);
    let next = options(value);
    if previous.as_ref().map(|snapshot| &snapshot.options) == next.as_ref() {
        return;
    }
    if previous
        .as_ref()
        .map(|snapshot| &snapshot.options.model.current)
        != next.as_ref().map(|options| &options.model.current)
    {
        record["agy_context"] = Value::Null;
    }
    record["agy_config"] = next.map_or(Value::Null, |options| {
        json!(Snapshot {
            revision: uuid::Uuid::new_v4().to_string(),
            options
        })
    });
}

pub(super) fn reset(record: &mut Value) {
    for field in [
        "agy_config_scope",
        "agy_config",
        "agy_settings_pending",
        "agy_context",
        "agy_last_turn_usage",
    ] {
        record
            .as_object_mut()
            .expect("Antigravity record")
            .remove(field);
    }
}

pub(super) fn project(row: &mut Value, record: &Value, live: bool, idle: bool, detail: bool) {
    row["agy_quota"] = json!({"status":"unavailable","reason":"provider_interface_unavailable"});
    let current = live.then(|| snapshot(record)).flatten();
    let Some(Snapshot { revision, options }) = current else {
        let reason = if live && !same_scope(record) {
            "legacy_worker"
        } else {
            "settings_unreported"
        };
        row["agy_settings"] = json!({"status":"unavailable","reason":reason});
        return;
    };
    let Options {
        model,
        effort,
        mode,
        legacy,
    } = options;
    let current_effort = effort.as_ref().map(|effort| effort.current.as_str());
    let pending = record["agy_settings_pending"] == true;
    let can_change = idle && !legacy && !pending;
    let model_change = can_change && model.choices.len() > 1;
    let effort_change = can_change
        && effort
            .as_ref()
            .is_some_and(|effort| effort.choices.len() > 1);
    row["agy_settings"] = json!({
        "status":if legacy {"legacy"} else if pending {"unavailable"} else {"confirmed"},
        "revision":if legacy || pending { None } else { Some(revision) },
        "mode":if pending { None } else { mode.as_ref().and_then(Select::current) },
        "reason":if pending {"awaiting_native_result"} else if legacy {"read_only_legacy"} else if !idle {"active_turn"} else {""}
    });
    if pending {
        return;
    }
    row["model"] = json!(model.current);
    row["settings_model"] = row["model"].clone();
    row["effort"] = json!(current_effort.unwrap_or_default());
    row["settings_change_supported"] = json!(model_change || effort_change);
    row["model_change_supported"] = json!(model_change);
    row["effort_change_supported"] = json!(effort_change);
    if model_change || effort_change {
        row["settings_apply_when"] = json!("now");
    }
    if detail {
        row["model_options"] = json!(
            model
                .choices
                .iter()
                .map(|choice| {
                    let mut item = json!({"id":choice.id,"label":choice.label});
                    if choice.id == model.current {
                        if let Some(effort) = &effort {
                            item["effort_options"] = json!(
                                effort
                                    .choices
                                    .iter()
                                    .map(|choice| &choice.id)
                                    .collect::<Vec<_>>()
                            );
                            item["effort_labels"] = json!(
                                effort
                                    .choices
                                    .iter()
                                    .map(|choice| (&choice.id, &choice.label))
                                    .collect::<std::collections::BTreeMap<_, _>>()
                            );
                        }
                    }
                    item
                })
                .collect::<Vec<_>>()
        );
    }
}

pub(super) fn usage_update(usage: &Value) -> Value {
    let (Some(used), Some(size)) = (usage["used"].as_u64(), usage["size"].as_u64()) else {
        return Value::Null;
    };
    if size == 0 {
        return Value::Null;
    }
    json!({"used":used,"limit":size,"used_source":"agy_acp","limit_source":"agy_acp"})
}

pub(super) fn turn_usage(usage: &Value) -> Value {
    let token_fields = [
        ("inputTokens", "input_tokens"),
        ("outputTokens", "output_tokens"),
        ("cachedReadTokens", "cached_read_tokens"),
        ("cachedWriteTokens", "cached_write_tokens"),
        ("thoughtTokens", "thought_tokens"),
        ("totalTokens", "total_tokens"),
    ];
    let fields: serde_json::Map<_, _> = token_fields
        .into_iter()
        .filter_map(|(wire, field)| {
            usage[wire]
                .as_u64()
                .map(|tokens| (field.into(), json!(tokens)))
        })
        .collect();
    if fields.is_empty() {
        Value::Null
    } else {
        Value::Object(fields)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub request_id: String,
    pub expected_run_id: String,
    pub expected_conversation_id: String,
    #[serde(default)]
    pub expected_session_id: Option<String>,
    pub expected_config_revision: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub effort: String,
}

impl Request {
    pub fn valid(&self) -> bool {
        [
            &self.request_id,
            &self.expected_run_id,
            &self.expected_conversation_id,
            &self.expected_config_revision,
        ]
        .into_iter()
        .all(|id| session_id::canonical(id))
            && self
                .expected_session_id
                .as_ref()
                .is_none_or(|id| session_id::canonical(id))
            && valid_id(&self.model)
            && (self.effort.is_empty() || valid_id(&self.effort))
    }

    pub fn matches(&self, record: &Value) -> bool {
        record["run_id"] == self.expected_run_id
            && record["conversation_id"] == self.expected_conversation_id
            && self
                .expected_session_id
                .as_ref()
                .is_none_or(|id| record["session_id"] == *id)
    }

    pub fn select(&self, record: &Value) -> Result<Option<(String, String)>> {
        let Snapshot { revision, options } =
            snapshot(record).ok_or("Antigravity has not reported model controls for this run")?;
        if options.legacy || revision != self.expected_config_revision {
            return Err(
                "Antigravity model options changed; refresh before applying settings".into(),
            );
        }
        if !options.model.offers(&self.model) {
            return Err("That model is no longer offered by Antigravity".into());
        }
        if self.effort.is_empty() {
            return Ok((self.model != options.model.current)
                .then_some((options.model.id, self.model.clone())));
        }
        if self.model != options.model.current {
            return Err("Apply the Antigravity model before selecting its thinking level".into());
        }
        let effort = options
            .effort
            .filter(|option| option.offers(&self.effort))
            .ok_or("That thinking level is not offered for this Antigravity model")?;
        Ok((self.effort != effort.current).then_some((effort.id, self.effort.clone())))
    }

    pub fn confirmed(&self, record: &Value) -> Option<Value> {
        let snapshot = snapshot(record)?;
        let options = snapshot.options;
        let effort = options
            .effort
            .as_ref()
            .map(|effort| effort.current.as_str())
            .unwrap_or_default();
        (!options.legacy
            && options.model.current == self.model
            && (self.effort.is_empty() || self.effort == effort))
            .then(|| {
                let mut answer = self.answer(record, "applied", "");
                answer["effort"] = json!(effort);
                answer["config_revision"] = json!(snapshot.revision);
                answer["changed"] = json!(true);
                answer
            })
    }

    pub fn answer(&self, record: &Value, status: &str, reason: &str) -> Value {
        let session_id = self
            .expected_session_id
            .as_deref()
            .unwrap_or_else(|| string(record, "session_id"));
        json!({"status":status,"scope":"session","request_id":self.request_id,
            "name":record["name"],"run_id":self.expected_run_id,"conversation_id":self.expected_conversation_id,
            "session_id":session_id,"model":self.model,"effort":self.effort,"reason":reason})
    }

    fn receipt_path(&self) -> PathBuf {
        root()
            .join("agy-settings")
            .join(format!("{}.json", self.request_id))
    }

    fn request_digest(&self) -> String {
        digest(&serde_json::to_vec(self).expect("Antigravity settings request"))
    }

    pub fn receipt(&self, name: &str) -> Result<Option<Value>> {
        let Some(bytes) = labels::read_file(&self.receipt_path(), 8192)
            .map_err(|_| "The original Antigravity settings receipt is unreadable")?
        else {
            return Ok(None);
        };
        let mut answer: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "The original Antigravity settings receipt is invalid")?;
        if answer["digest"] != self.request_digest()
            || !session_id::canonical(string(&answer, "session_id"))
            || self
                .expected_session_id
                .as_ref()
                .is_some_and(|id| answer["session_id"] != *id)
            || answer["name"] != name
            || answer["run_id"] != self.expected_run_id
            || answer["conversation_id"] != self.expected_conversation_id
            || answer["request_id"] != self.request_id
            || !matches!(
                answer["status"].as_str(),
                Some("applied" | "refused" | "uncertain")
            )
        {
            return Err(
                "That request ID already belongs to another Antigravity settings attempt".into(),
            );
        }
        answer
            .as_object_mut()
            .expect("Antigravity settings receipt")
            .remove("digest");
        Ok(Some(answer))
    }

    pub fn save(&self, answer: &Value) -> Result<()> {
        let mut retained = answer.clone();
        retained["digest"] = json!(self.request_digest());
        atomic(&self.receipt_path(), &retained.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, category: &str) -> Value {
        json!({"id":id,"category":category,"type":"select","currentValue":"one",
            "options":[{"value":"one","name":"One"},{"value":"two","name":"Two"}]})
    }

    #[test]
    fn permission_mode_cannot_be_a_model_or_thinking_write() {
        assert!(options(&json!({"configOptions":[entry("mode", "model")]})).is_none());
        let parsed = options(&json!({
            "configOptions":[entry("model","model"),entry("mode","thought_level")]
        }))
        .unwrap();
        assert!(parsed.effort.is_none());
        assert!(parsed.mode.is_none());
    }

    #[test]
    fn selected_native_values_must_be_unique_and_offered() {
        let mut model = entry("model", "model");
        model["options"][1]["value"] = json!("one");
        assert!(options(&json!({"configOptions":[model.clone()]})).is_none());
        model["options"][1]["value"] = json!("two");
        model["currentValue"] = json!("removed");
        assert!(options(&json!({"configOptions":[model]})).is_none());
    }

    #[test]
    fn legacy_models_are_current_only_for_their_original_worker() {
        let mut record = json!({"run_id":"old","session_id":"session","conversation_id":"conversation",
            "agy_worker":{"pid":1,"birth":"original"}});
        bind_scope(&mut record);
        observe(
            &mut record,
            &json!({"models":{"currentModelId":"one","availableModels":[
                {"modelId":"one","name":"One"},{"modelId":"two","name":"Two"}
            ]}}),
        );
        let mut row = json!({});
        project(&mut row, &record, true, true, true);
        assert_eq!(row["agy_settings"]["status"], "legacy");
        assert_eq!(row["model"], "one");
        assert_eq!(row["settings_change_supported"], false);
        record["agy_worker"]["birth"] = json!("replacement");
        let mut row = json!({});
        project(&mut row, &record, true, true, true);
        assert_eq!(row["agy_settings"]["reason"], "legacy_worker");
        assert!(row.get("model").is_none());
    }
}
