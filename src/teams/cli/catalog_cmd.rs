//! The person's own catalog and templates. These commands are the CLI half of
//! the designer: they read and write the one JSON file a person owns, and they
//! create nothing in it until an explicit save or import.
use super::actor::{self, Caller};
use super::input::{self, Options};
use super::{request_input, storage};
use crate::config::Config;
use crate::teams::catalog;
use crate::teams::store::Store;
use serde_json::{json, Value};
use zerus_team_plan::catalog::{Catalog, Template, TemplateBundle};
use zerus_team_plan::problem::code;
use zerus_team_plan::Problem;

pub(super) fn run(
    config: &Config,
    caller: &Caller,
    group: &str,
    args: &[String],
    provided: Option<&str>,
) -> Result<Value, Problem> {
    let store = Store::open(&config.state)?;
    let identity = actor::resolve(&store, caller)?;
    actor::person_only(&identity, group)?;
    let (command, rest) = args
        .split_first()
        .ok_or_else(|| input::usage(format!("{group} needs a command")))?;
    match (group, command.as_str()) {
        ("catalog", "show") => catalog_of(config, &catalog::load(&config.dir)?),
        ("catalog", "export") => catalog_of(config, &catalog::person(&config.dir)?),
        ("catalog", "import") => import_catalog(config, provided),
        ("template", "list") => templates(config, rest),
        ("template", "show") => show(config, rest),
        ("template", "save") => save(config, provided),
        ("template", "import") => import_bundle(config, provided),
        ("template", "export") => export(config, rest),
        ("template", "remove") => remove(config, rest),
        _ => Err(super::unknown(&format!("{group} {command}"))),
    }
}

fn catalog_of(_config: &Config, catalog: &Catalog) -> Result<Value, Problem> {
    Ok(json!({"ok": true, "catalog": serde_json::to_value(catalog).map_err(storage)?}))
}

fn words(args: &[String]) -> Result<Options, Problem> {
    Options::parse(args, &[], &[])
}

fn list_reply(catalog: &Catalog) -> Result<Value, Problem> {
    let rows: Vec<Value> = catalog
        .templates
        .iter()
        .map(|template| {
            json!({
                "id": template.id,
                "name": template.name,
                "description": template.description,
            })
        })
        .collect();
    Ok(json!({"ok": true, "templates": rows}))
}

fn templates(config: &Config, args: &[String]) -> Result<Value, Problem> {
    words(args)?.nothing_else("template list")?;
    list_reply(&catalog::load(&config.dir)?)
}

fn show(config: &Config, args: &[String]) -> Result<Value, Problem> {
    let options = words(args)?;
    let id = options.required("a template ID")?;
    let catalog = catalog::load(&config.dir)?;
    let template = catalog
        .templates
        .iter()
        .find(|template| template.id == id)
        .ok_or_else(|| unknown_template(id))?;
    Ok(json!({"ok": true, "template": serde_json::to_value(template).map_err(storage)?}))
}

fn export(config: &Config, args: &[String]) -> Result<Value, Problem> {
    let options = words(args)?;
    let id = options.required("a template ID")?;
    let catalog = catalog::load(&config.dir)?;
    let bundle = catalog::export_bundle(&catalog, id)?;
    Ok(json!({"ok": true, "bundle": serde_json::to_value(bundle).map_err(storage)?}))
}

fn remove(config: &Config, args: &[String]) -> Result<Value, Problem> {
    let options = words(args)?;
    let id = options.required("a template ID")?;
    let mut person = catalog::person(&config.dir)?;
    let Some(index) = person
        .templates
        .iter()
        .position(|template| template.id == id)
    else {
        return Err(Problem::new(
            code::NOT_FOUND,
            "Only a template in the person's own catalog can be removed.",
        )
        .with_refs([id]));
    };
    person.templates.remove(index);
    catalog::save(&config.dir, &person)?;
    let built_in = catalog::load(&config.dir)?
        .templates
        .iter()
        .any(|template| template.id == id);
    Ok(json!({"ok": true, "removed": id, "still_available": built_in}))
}

fn save(config: &Config, provided: Option<&str>) -> Result<Value, Problem> {
    let template: Template = parsed(&request_input(provided)?, "template")?;
    let mut person = catalog::person(&config.dir)?;
    match person
        .templates
        .iter()
        .position(|saved| saved.id == template.id)
    {
        Some(index) => person.templates[index] = template.clone(),
        None => person.templates.push(template.clone()),
    }
    catalog::save(&config.dir, &person)?;
    Ok(json!({"ok": true, "saved": template.id}))
}

fn import_bundle(config: &Config, provided: Option<&str>) -> Result<Value, Problem> {
    let bundle: TemplateBundle = parsed(&request_input(provided)?, "template bundle")?;
    catalog::import_bundle(&config.dir, &bundle)?;
    Ok(json!({"ok": true, "imported": bundle.template.id}))
}

fn import_catalog(config: &Config, provided: Option<&str>) -> Result<Value, Problem> {
    let catalog: Catalog = parsed(&request_input(provided)?, "catalog")?;
    catalog::save(&config.dir, &catalog)?;
    Ok(json!({"ok": true, "saved": true}))
}

/// One JSON document of the type the command stores, with an unknown field
/// named by its own code.
fn parsed<T: serde::de::DeserializeOwned>(text: &str, what: &str) -> Result<T, Problem> {
    serde_json::from_value(input::value(text, what)?)
        .map_err(|error| input::serde_problem(error.to_string()))
}

fn unknown_template(id: &str) -> Problem {
    Problem::new(code::NOT_FOUND, "Template does not exist.").with_refs([id])
}
