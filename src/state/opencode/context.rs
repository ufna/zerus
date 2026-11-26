//! OpenCode reports aggregate session tokens and per-step tokens separately.
use super::*;

fn number(value: &Value) -> Option<u64> {
    value.as_u64().filter(|&n| n <= 9_007_199_254_740_991)
}

fn sum(values: &[Option<u64>]) -> Option<u64> {
    values
        .iter()
        .try_fold(0u64, |total, part| total.checked_add((*part)?))
        .filter(|&n| n <= 9_007_199_254_740_991)
}

fn tokens(native: &Value) -> Value {
    let uncached = number(&native["input"]);
    let read = number(&native["cache"]["read"]);
    let write = number(&native["cache"]["write"]);
    let reasoning = number(&native["reasoning"]);
    let input = sum(&[uncached, read, write]);
    // OpenCode normalizes visible output and reasoning into disjoint buckets.
    let output = sum(&[number(&native["output"]), reasoning]);
    json!({"uncached_input":uncached,"cache_read":read,"cache_write":write,
        "reasoning":reasoning,"input":input,"output":output,"total":sum(&[input,output])})
}

pub(super) fn model_limits(catalog: &Value, selected: &str) -> Value {
    let Some((provider, id)) = selected.split_once('/') else {
        return Value::Null;
    };
    let mut matches = catalog["data"]
        .as_array()
        .into_iter()
        .flatten()
        .take(catalog::MAX_FULL)
        .filter(|model| {
            model["providerID"] == provider && model["id"] == id && model["enabled"] != false
        });
    let Some(model) = matches.next() else {
        return Value::Null;
    };
    if matches.next().is_some() {
        return Value::Null;
    }
    let positive = |key| number(&model["limit"][key]).filter(|&n| n > 0);
    json!({"limit":positive("context"),"output_limit":positive("output")})
}

fn same_model(left: &Value, right: &Value) -> bool {
    ["providerID", "id"].into_iter().all(|key| {
        left[key]
            .as_str()
            .is_some_and(|value| !value.is_empty() && right[key] == value)
    })
}

/// Use only the newest native completed model step in the current context.
/// A summary call reads the *old* context. Its tokens belong in the session
/// total, but cannot measure the new context until the next model step.
fn last_step<'a>(messages: &'a [Value], model: &Value) -> Option<&'a Value> {
    for message in messages.iter().rev() {
        match string(message, "type") {
            "model-switched" => return None,
            "compaction" if message["status"] == "completed" => return None,
            "assistant"
                if number(&message["time"]["completed"]).is_some_and(|time| time > 0)
                    && matches!(string(message, "finish"), "stop" | "length" | "tool-calls")
                    && message["error"].is_null()
                    && message["retry"].is_null() =>
            {
                return same_model(&message["model"], model).then_some(message);
            }
            _ => {}
        }
    }
    None
}

/// A completed summary is coherent with an aggregate sample only once the
/// session has left that turn. The summary row may appear before that sample.
pub(super) fn compact_boundary(messages: &[Value], attempt: &Value) -> Option<u64> {
    let time = |value: &Value| number(value).filter(|&time| time > 0).unwrap_or(u64::MAX);
    let history = messages
        .iter()
        .filter(|message| message["type"] == "compaction" && message["status"] == "completed")
        .map(|message| time(&message["time"]["created"]));
    let exact = (attempt["status"] == "completed").then(|| time(&attempt["native_created_at"]));
    history.chain(exact).max()
}

pub(super) fn covers_compact(session: &Value, boundary: Option<u64>) -> bool {
    boundary.is_none_or(|boundary| {
        number(&session["time"]["idle"]).is_some_and(|idle| idle >= boundary)
    })
}

pub(super) fn usage(
    session: &Value,
    messages: &[Value],
    limits: &Value,
    boundary: Option<u64>,
) -> Value {
    let current = covers_compact(session, boundary);
    let last = current
        .then(|| last_step(messages, &session["model"]))
        .flatten()
        .filter(|message| {
            boundary.is_none_or(|boundary| {
                number(&message["time"]["completed"]).is_some_and(|step| step > boundary)
            })
        });
    let step = last
        .map(|message| tokens(&message["tokens"]))
        .unwrap_or(Value::Null);
    let used = number(&step["total"]).filter(|&total| total > 0);
    let totals = tokens(&session["tokens"]);
    // A new session has zero-filled counters before OpenCode calls a model.
    let has_usage = current
        && totals.as_object().is_some_and(|values| {
            values
                .values()
                .any(|value| number(value).is_some_and(|n| n > 0))
        });
    let cost = session["cost"]
        .as_f64()
        .filter(|cost| has_usage && cost.is_finite() && *cost >= 0.0);
    let cache = if used.is_some() {
        json!({"status":"unknown","source":"opencode_native_step",
            "observed_at":number(&last.expect("reported step")["time"]["completed"]).map(|t| t as f64/1000.0),
            "cache_read":step["cache_read"],"cache_write":step["cache_write"],"input":step["input"]})
    } else {
        Value::Null
    };
    json!({"status":if has_usage || used.is_some() {"ok"} else {"unavailable"},
        "source":"opencode","scope":"conversation","includes_subagents":false,
        "totals":has_usage.then_some(totals),"last_request":used.map(|_|step),"cost_usd":cost,
        "prompt_cache":cache,"context":{"used":used,"limit":limits["limit"],"output_limit":limits["output_limit"],
            "limit_source":limits["limit"].is_number().then_some("opencode_model_catalog"),
            "used_source":used.map(|_|"opencode_completed_step")}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_limits_require_the_exact_offered_model_and_keep_output_separate() {
        let catalog = json!({"data":[
            {"providerID":"p","id":"nested/model","limit":{"context":32768,"output":2048}},
            {"providerID":"q","id":"nested/model","limit":{"context":65536,"output":4096}},
            {"providerID":"p","id":"unknown","limit":{"output":8192}},
            {"providerID":"p","id":"ambiguous","limit":{"context":111}},
            {"providerID":"p","id":"ambiguous","limit":{"context":222}}
        ]});
        assert_eq!(
            model_limits(&catalog, "p/nested/model"),
            json!({"limit":32768,"output_limit":2048})
        );
        assert_eq!(
            model_limits(&catalog, "p/unknown"),
            json!({"limit":null,"output_limit":8192})
        );
        assert_eq!(model_limits(&catalog, "p/ambiguous"), Value::Null);
        assert_eq!(model_limits(&catalog, "p/model"), Value::Null);
    }

    #[test]
    fn footprint_stops_at_a_completed_summary_or_switch_without_changing_totals() {
        let model = json!({"id":"local","providerID":"fixture"});
        let session = json!({"model":model,"tokens":{"input":500,"output":40,"reasoning":20,
            "cache":{"read":100,"write":10}}});
        let complete = json!({"type":"assistant","model":model,"time":{"completed":1000},"finish":"stop",
            "tokens":{"input":100,"output":20,"reasoning":10,"cache":{"read":200,"write":5}}});
        let streaming = json!({"type":"assistant","model":model,"time":{"created":1001},
            "tokens":{"input":900,"output":0,"reasoning":0,"cache":{"read":0,"write":0}}});
        let mut error = complete.clone();
        error["finish"] = json!("error");
        error["error"] = json!({"_tag":"APIError"});
        error["tokens"]["input"] = json!(800);
        let mut retry = complete.clone();
        retry["retry"] = json!({"attempt":1,"at":1002,"error":{"_tag":"APIError"}});
        retry["tokens"]["input"] = json!(1000);
        let mut tool_step = complete.clone();
        tool_step["finish"] = json!("tool-calls");
        tool_step["tokens"]["input"] = json!(400);
        let limits = json!({"limit":16000,"output_limit":1000});
        let view = |messages: &[Value]| usage(&session, messages, &limits, None);
        let measured = view(&[complete.clone(), streaming, error.clone(), retry.clone()]);
        assert_eq!(measured["context"]["used"], 335);
        assert_eq!(measured["totals"]["total"], 670);
        assert_eq!(measured["totals"]["output"], 60);
        for boundary in [
            json!({"type":"compaction","status":"completed","reason":"manual","tokens":session["tokens"]}),
            json!({"type":"model-switched","model":model}),
            json!({"type":"assistant","model":{"providerID":"other","id":"local"},"time":{"completed":1002},"finish":"stop","tokens":session["tokens"]}),
        ] {
            let refreshed = view(&[
                complete.clone(),
                boundary.clone(),
                error.clone(),
                retry.clone(),
            ]);
            assert!(refreshed["context"]["used"].is_null());
            assert!(refreshed["prompt_cache"].is_null());
            assert_eq!(refreshed["totals"], measured["totals"]);
            let next = view(&[boundary, error.clone(), tool_step.clone(), retry.clone()]);
            assert_eq!(next["context"]["used"], 635);
            assert_eq!(next["totals"], measured["totals"]);
        }
    }

    #[test]
    fn a_summary_newer_than_the_native_idle_sample_keeps_only_limits() {
        let mut session = json!({"time":{"idle":500},"tokens":{"input":100,"output":20,"reasoning":10,
            "cache":{"read":200,"write":5}}});
        let limits = json!({"limit":16000,"output_limit":1000});
        let receipt = json!({"status":"completed","native_created_at":600});
        let boundary = compact_boundary(&[], &receipt);
        assert_eq!(boundary, Some(600));
        let stale = usage(&session, &[], &limits, boundary);
        assert!(stale["totals"].is_null());
        assert!(stale["context"]["used"].is_null());
        assert_eq!(stale["context"]["limit"], 16000);
        session["time"]["idle"] = json!(650);
        assert_eq!(
            usage(&session, &[], &limits, boundary)["totals"]["total"],
            335
        );
        assert!(!covers_compact(
            &session,
            compact_boundary(
                &[json!({"type":"compaction","status":"completed"})],
                &Value::Null
            )
        ));
    }
}
