//! Person-confirmed local copy review, separate from every task execution path.
use super::{actor, fail, machines, option, project_arg, Result};
use crate::config::Config;
use serde_json::{json, Value};
use std::time::Duration;
use zerus_replica::ProjectStore;

pub(super) fn execute(config: &Config, args: &[String]) -> Result<Value> {
    let mut index = 0;
    let mut seen = std::collections::BTreeSet::new();
    while index < args.len() {
        if !seen.insert(args[index].as_str()) {
            return Err(fail("review-copy refuses duplicate options"));
        }
        match args[index].as_str() {
            "--json" => index += 1,
            "--project" | "--uuid" | "--confirm" if index + 1 < args.len() => index += 2,
            _ => return Err(fail("usage: hgs task review-copy --project P --uuid UUID [--confirm FINGERPRINT] --json")),
        }
    }
    if !seen.contains("--project") || !seen.contains("--uuid") {
        return Err(fail("review-copy requires --project and the exact --uuid"));
    }
    let who = actor(&machines(config)?.here, args, false)?;
    let (id, name) = project_arg(config, args, &who)?;
    let uuid =
        option(args, "--uuid")?.ok_or_else(|| fail("review-copy requires the exact task UUID"))?;
    let confirmation = option(args, "--confirm")?;
    // The person, or an agent carrying out the person's own request
    // (zerus-46l1q), whose identity the reply records.
    let person = match confirmation {
        Some(_) => super::person_request_for(
            &who,
            "only a person, or an agent at the person's own request, confirms moving a damaged local task copy aside",
        )?,
        None => None,
    };
    let mut store =
        ProjectStore::open_copy_review(&config.state.join("replica"), &id, Duration::from_secs(2))
            .map_err(fail)?
            .ok_or_else(|| fail("the task store is busy; nothing was moved"))?;
    let mut reply =
        json!({"abi":"task-copy-review-v1","project":{"id":id,"name":name},"uuid":uuid});
    match confirmation {
        None => {
            let review = store.prepare_copy_review(uuid).map_err(fail)?;
            reply["state"] = json!("review");
            reply["fingerprint"] = json!(review.fingerprint);
            reply["files"] = json!(review.files);
        }
        Some(expected) => {
            if expected.len() != 64
                || !expected
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(fail("review-copy requires its exact review fingerprint"));
            }
            if let Some(person) = &person {
                reply["person_request"] = person.identity();
            }
            // Who confirmed it, and at whose request, is written durably
            // with the move (review S2-3). The request is checked again
            // after the review and its hashing, right before that record
            // and the move, and held through them (review D3).
            let reference = person.as_ref().map(|person| person.reference());
            let refused = std::cell::RefCell::new(None);
            let moved =
                store.move_reviewed_copy_admitted(uuid, expected, &who, reference.as_ref(), || {
                    super::recheck_held(person.as_ref(), uuid, super::person_deadline()).map_err(
                        |error| {
                            let message = error.message.clone();
                            *refused.borrow_mut() = Some(error);
                            message
                        },
                    )
                });
            if let Some(error) = refused.into_inner() {
                return Err(error);
            }
            match moved {
                Ok(outcome) => {
                    reply["state"] = json!(outcome.state);
                    reply["backup"] = json!(outcome.backup);
                    reply["reason"] = json!(outcome.reason);
                }
                Err(error) => {
                    reply["state"] = json!("refused");
                    reply["reason"] = json!(error.to_string());
                }
            }
        }
    }
    Ok(reply)
}
