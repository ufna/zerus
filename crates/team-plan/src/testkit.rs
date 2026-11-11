//! Small builders for the kernel's unit tests. Values come from JSON so every
//! field a test does not name takes its serde default, as a stored row would.

use serde_json::{json, Value};

use crate::model::{Node, NodeId};

/// Applies `fields` over `base` and parses the result.
fn build<T: serde::de::DeserializeOwned>(mut base: Value, fields: Value) -> T {
    let object = base.as_object_mut().expect("an object");
    for (key, value) in fields.as_object().expect("an object") {
        object.insert(key.clone(), value.clone());
    }
    serde_json::from_value(base).expect("a valid test value")
}

/// A pending developer task in planned phase 1 with the given fields over it.
pub fn node(id: NodeId, fields: Value) -> Node {
    build(
        json!({"id": id, "phase": 1, "title": format!("Task {id}"), "role": "developer", "state": "pending"}),
        fields,
    )
}
