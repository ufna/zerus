//! The New team form's catalog and the template save receipt.
//!
//! `hgs team catalog show --json` answers `{"ok": true, "catalog": ...}` with
//! the catalog serialized by `zerus_team_plan::catalog::Catalog`, which leaves
//! out empty and default fields. The reply is read into that same type through
//! the tolerant reader, so a malformed entry (a template without a roster or
//! settings, a role without an agent, a stage of an unknown kind) refuses the
//! whole catalog before the form sees it, a field this version does not know is
//! ignored at every level, and the catalog is given back with every field the
//! form reads present.
//! Each template keeps the `digest` that `catalog show` adds beside the
//! catalog type, so the form can bind a create to the template it showed.
use serde::Serialize;
use serde_json::{json, Value};
use zerus_team_plan::catalog::{Catalog, LeadSpec, Pipeline, Role, Template};

fn value<T: Serialize>(field: &T) -> Value {
    serde_json::to_value(field).unwrap_or(Value::Null)
}

fn role(role: &Role) -> Value {
    json!({
        "id": role.id, "name": role.name, "agent": role.agent, "account": value(&role.account),
        "model": role.model, "effort": role.effort, "pipeline": role.pipeline, "context": value(&role.context),
    })
}

fn pipeline(pipeline: &Pipeline) -> Value {
    let stages: Vec<Value> = pipeline
        .stages
        .iter()
        .map(|stage| json!({"kind": value(&stage.kind), "role": stage.role}))
        .collect();
    json!({
        "id": pipeline.id, "name": pipeline.name, "implementer": pipeline.implementer, "stages": stages,
        "max_rounds": pipeline.max_rounds, "accept": value(&pipeline.accept),
    })
}

fn lead(lead: &LeadSpec) -> Value {
    json!({
        "agent": lead.agent, "account": value(&lead.account), "model": lead.model,
        "effort": lead.effort, "rules": lead.rules,
    })
}

fn template(template: &Template, digest: Option<String>) -> Value {
    let roster: Vec<Value> = template
        .roster
        .iter()
        .map(|seat| json!({"role": seat.role, "count": seat.count, "optional": seat.optional}))
        .collect();
    json!({
        "id": template.id, "name": template.name, "description": template.description, "lead": lead(&template.lead),
        "roster": roster, "default_pipeline": template.default_pipeline, "settings": value(&template.settings),
        "digest": digest,
    })
}

/// The longest digest kept; the command line's is 64 hex characters.
const DIGEST_CHARS: usize = 128;

/// Takes each template row's `digest` out of a raw catalog, in order. The
/// command line adds it to `catalog show` only, outside the catalog type, so
/// the rest is read as that type unchanged. It is opaque: kept as it came
/// when it is a short word, and absent otherwise (an older command line
/// sends none), which leaves the form unable to bind the template.
fn take_digests(raw: &mut Value) -> Vec<Option<String>> {
    let Some(templates) = raw.get_mut("templates").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    templates
        .iter_mut()
        .map(|row| {
            let digest = row.as_object_mut()?.remove("digest")?;
            let text = digest.as_str()?;
            (!text.is_empty()
                && text.len() <= DIGEST_CHARS
                && !text.chars().any(|c| c.is_control() || c.is_whitespace()))
            .then(|| text.to_owned())
        })
        .collect()
}

/// The form's catalog from a `catalog show` reply: a success whose catalog is
/// the command line's own catalog type, every template with an id, and every
/// field the form reads present. Anything else is refused with its reason.
pub fn form_catalog(reply: &Value) -> Result<Value, String> {
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err("The catalog answer is not a success.".into());
    }
    let raw = reply
        .get("catalog")
        .filter(|catalog| catalog.is_object())
        .ok_or("The catalog answer has no catalog.")?;
    let mut raw = raw.clone();
    let digests = take_digests(&mut raw);
    // Read as the command line's own type, ignoring at every level what this
    // version does not know: a field the store adds later never refuses the
    // catalog of an installed client (the type stays strict for save and import).
    let catalog: Catalog = super::tolerant::read(raw)
        .map_err(|error| format!("The catalog answer is not a catalog: {error}."))?;
    if catalog
        .templates
        .iter()
        .any(|template| template.id.is_empty())
    {
        return Err("The catalog has a template without an id.".into());
    }
    let roles: Vec<Value> = catalog.roles.iter().map(role).collect();
    let pipelines: Vec<Value> = catalog.pipelines.iter().map(pipeline).collect();
    let templates: Vec<Value> = catalog
        .templates
        .iter()
        .enumerate()
        .map(|(index, entry)| template(entry, digests.get(index).cloned().flatten()))
        .collect();
    Ok(json!({
        "ok": true,
        "catalog": {"roles": roles, "pipelines": pipelines, "templates": templates, "planning_rules": catalog.planning_rules},
    }))
}

/// A template save's receipt: a success that names the template that was sent.
pub fn saved_receipt(reply: &Value, sent: &str) -> Result<Value, String> {
    let saved = reply
        .get("saved")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if reply.get("ok").and_then(Value::as_bool) != Some(true) || saved.is_empty() {
        return Err("The template was not saved: the answer named no template.".into());
    }
    if saved != sent {
        return Err(format!(
            "The template save named {saved}, not the template that was sent."
        ));
    }
    Ok(json!({"ok": true, "saved": saved}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin() -> Value {
        serde_json::from_str(include_str!(
            "../../../../crates/team-plan/fixtures/catalog-builtin.json"
        ))
        .unwrap()
    }

    /// The command line's own `catalog show` reply with the digests
    /// (`crates/team-plan/fixtures/cli-catalog.json`).
    fn shown() -> Value {
        serde_json::from_str(include_str!(
            "../../../../crates/team-plan/fixtures/cli-catalog.json"
        ))
        .unwrap()
    }

    #[test]
    fn catalog_show_keeps_each_templates_digest_in_its_row() {
        let reply = shown();
        let form = form_catalog(&reply).expect("a catalog with digests is read");
        let sent: Vec<(&str, &str)> = reply["catalog"]["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["id"].as_str().unwrap(), row["digest"].as_str().unwrap()))
            .collect();
        let kept: Vec<(&str, &str)> = form["catalog"]["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["id"].as_str().unwrap(), row["digest"].as_str().unwrap()))
            .collect();
        assert_eq!(kept, sent);
        assert!(kept.contains(&(
            "bugfix",
            "6d15fdbdd92ac4d5c5bc53f0bd6da7871c0dbf1842f4d4f25bf94aebc95bbb9d"
        )));
    }

    #[test]
    fn a_catalog_without_digests_is_read_with_none_so_nothing_can_be_bound() {
        let form = form_catalog(&json!({"ok": true, "catalog": builtin()})).unwrap();
        for row in form["catalog"]["templates"].as_array().unwrap() {
            assert!(row["digest"].is_null(), "{row}");
        }
        let mut reply = shown();
        let templates = reply["catalog"]["templates"].as_array_mut().unwrap();
        templates[0]["digest"] = json!(12);
        templates[1]["digest"] = json!("a b");
        templates[2]["digest"] = json!("x".repeat(129));
        let form = form_catalog(&reply).expect("a malformed digest leaves the catalog readable");
        for row in form["catalog"]["templates"].as_array().unwrap() {
            assert!(row["digest"].is_null(), "{row}");
        }
    }

    #[test]
    fn the_command_lines_catalog_comes_back_with_every_field_the_form_reads() {
        let form = form_catalog(&json!({"ok": true, "catalog": builtin()})).unwrap();
        let catalog = &form["catalog"];
        assert!(!catalog["templates"].as_array().unwrap().is_empty());
        for template in catalog["templates"].as_array().unwrap() {
            for key in ["id", "name", "description", "default_pipeline"] {
                assert!(template[key].is_string(), "{key} of {template}");
            }
            assert!(template["roster"].is_array() && template["lead"]["agent"].is_string());
            for key in [
                "integration",
                "plan_approval",
                "phase_acceptance",
                "routing",
            ] {
                assert!(template["settings"][key].is_string(), "{key}");
            }
            assert!(template["settings"]["queue"].is_boolean());
            assert!(template["settings"]["land_allowed"].is_boolean());
        }
        for role in catalog["roles"].as_array().unwrap() {
            for key in [
                "id", "name", "agent", "model", "effort", "pipeline", "context",
            ] {
                assert!(role[key].is_string(), "{key} of {role}");
            }
            assert!(role["account"]["mode"].is_string());
        }
        for pipeline in catalog["pipelines"].as_array().unwrap() {
            for stage in pipeline["stages"].as_array().unwrap() {
                assert!(stage["kind"].is_string() && stage["role"].is_string());
            }
        }
    }

    #[test]
    fn an_empty_catalog_is_three_empty_arrays() {
        let form = form_catalog(&json!({"ok": true, "catalog": {}})).unwrap();
        for key in ["roles", "pipelines", "templates", "planning_rules"] {
            assert_eq!(form["catalog"][key], json!([]), "{key}");
        }
    }

    #[test]
    fn a_malformed_entry_refuses_the_catalog() {
        let refused =
            |catalog: Value| form_catalog(&json!({"ok": true, "catalog": catalog})).is_err();
        assert!(form_catalog(&json!({})).is_err());
        assert!(form_catalog(&json!({"ok": true})).is_err());
        assert!(refused(
            json!({"templates": [{"id": "broken", "name": "Broken"}]})
        ));
        assert!(refused(json!({"templates": [{"name": "No id"}]})));
        let mut template = builtin()["templates"][0].clone();
        template["id"] = json!("");
        assert!(refused(json!({"templates": [template]})));
        assert!(
            refused(json!({"roles": [{"id": "r", "name": "R"}]})),
            "a role without an agent"
        );
        assert!(refused(
            json!({"pipelines": [{"id": "p", "name": "P", "implementer": "r", "stages": [{"kind": "deploy", "role": "r"}]}]})
        ));
        assert!(refused(json!({"roles": "not a list"})));
        // A field this version does not know is not malformed: it is ignored
        // (teams::tolerant).
        assert!(!refused(json!({"templates": [], "unknown": true})));
    }

    #[test]
    fn a_save_receipt_names_the_template_that_was_sent() {
        assert!(saved_receipt(&json!({"ok": true, "saved": "bugfix-mine"}), "bugfix-mine").is_ok());
        assert!(saved_receipt(&json!({"ok": true, "saved": "other"}), "bugfix-mine").is_err());
        assert!(saved_receipt(&json!({"ok": true}), "bugfix-mine").is_err());
        assert!(
            saved_receipt(&json!({"ok": false, "saved": "bugfix-mine"}), "bugfix-mine").is_err()
        );
    }
}
