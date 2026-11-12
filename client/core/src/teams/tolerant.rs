//! Reading a reply as the plan decides ("A reader of a reply ignores what it
//! does not know", decisions during wave 3): what is sent stays strict, and
//! what is read ignores unknown fields at every level.
//!
//! The command line's types refuse unknown fields, so that a save or an import
//! never drops one. A reply read through [`read`] gives every struct, at every
//! level, only the fields it declares, so a field the store adds later is
//! ignored instead of refusing the whole reply in a client that is already
//! installed. Everything else is still refused: a missing field, a value of
//! the wrong type, an unknown enum variant. Maps keep all their entries (their
//! keys are data, not field names).
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::forward_to_deserialize_any;
use serde_json::{Error, Value};

/// `T` from a reply, ignoring the fields `T` does not declare at any level.
pub fn read<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
    T::deserialize(Tolerant(value))
}

struct Tolerant(Value);

impl<'de> de::Deserializer<'de> for Tolerant {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Array(items) => visitor.visit_seq(Items(items.into_iter())),
            Value::Object(map) => visitor.visit_map(Entries::new(map.into_iter().collect())),
            other => de::Deserializer::deserialize_any(other, visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(Tolerant(other)),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Value::Object(map) => visitor.visit_map(Entries::new(
                map.into_iter()
                    .filter(|(key, _)| fields.contains(&key.as_str()))
                    .collect(),
            )),
            other => Tolerant(other).deserialize_any(visitor),
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Value::String(variant) => visitor.visit_enum(Variant {
                name: variant,
                content: None,
            }),
            Value::Object(map) if map.len() == 1 => {
                let (variant, content) = map.into_iter().next().expect("one entry");
                visitor.visit_enum(Variant {
                    name: variant,
                    content: Some(content),
                })
            }
            other => de::Deserializer::deserialize_enum(other, name, variants, visitor),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map identifier ignored_any
    }
}

struct Items(std::vec::IntoIter<Value>);

impl<'de> SeqAccess<'de> for Items {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.0
            .next()
            .map(|item| seed.deserialize(Tolerant(item)))
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

struct Entries {
    entries: std::vec::IntoIter<(String, Value)>,
    value: Option<Value>,
}

impl Entries {
    fn new(entries: Vec<(String, Value)>) -> Self {
        Self {
            entries: entries.into_iter(),
            value: None,
        }
    }
}

impl<'de> MapAccess<'de> for Entries {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match self.entries.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(Value::String(key)).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let value = self
            .value
            .take()
            .ok_or_else(|| de::Error::custom("a value without its key"))?;
        seed.deserialize(Tolerant(value))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

struct Variant {
    name: String,
    content: Option<Value>,
}

impl<'de> EnumAccess<'de> for Variant {
    type Error = Error;
    type Variant = Content;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Content), Error> {
        let variant = seed.deserialize(Value::String(self.name))?;
        Ok((variant, Content(self.content)))
    }
}

struct Content(Option<Value>);

impl<'de> VariantAccess<'de> for Content {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.0 {
            None | Some(Value::Null) => Ok(()),
            Some(_) => Err(de::Error::custom("a unit variant carries no content")),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        match self.0 {
            Some(value) => seed.deserialize(Tolerant(value)),
            None => Err(de::Error::custom("a newtype variant needs its content")),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Some(Value::Array(items)) => visitor.visit_seq(Items(items.into_iter())),
            _ => Err(de::Error::custom("a tuple variant needs a list")),
        }
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Some(value) => {
                de::Deserializer::deserialize_struct(Tolerant(value), "", fields, visitor)
            }
            None => Err(de::Error::custom("a struct variant needs its fields")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Exit;
    use crate::teams::catalog::form_catalog;
    use crate::teams::read::{decode, TeamRead};
    use crate::teams::view::{list, team, waiting};
    use serde_json::json;

    const TEAM: &str = "00000000-0000-4000-8000-00000000beef";

    /// The command line's pinned replies (`crates/team-plan/fixtures`), and the
    /// storage problem the core keeps a copy of.
    fn fixture(name: &str) -> Value {
        serde_json::from_str(match name {
            "catalog" => include_str!("../../../../crates/team-plan/fixtures/cli-catalog.json"),
            "list" => include_str!("../../../../crates/team-plan/fixtures/cli-list.json"),
            "status" => include_str!("../../../../crates/team-plan/fixtures/cli-status.json"),
            "queue" => include_str!("../../../../crates/team-plan/fixtures/cli-queue.json"),
            "cards" => include_str!("../../../../crates/team-plan/fixtures/cli-cards.json"),
            "waiting" => include_str!("../../../../crates/team-plan/fixtures/cli-waiting.json"),
            "template-changed" => {
                include_str!("../../../../crates/team-plan/fixtures/cli-template-changed.json")
            }
            "storage-failed" => include_str!("fixtures/cli-storage-failed.json"),
            other => panic!("no fixture {other}"),
        })
        .unwrap()
    }

    /// The same reply with a field this version does not know in every object,
    /// at every level, and how many objects received it.
    fn with_unknown_everywhere(value: &Value) -> (Value, usize) {
        match value {
            Value::Object(map) => {
                let mut added = 1;
                let mut out = serde_json::Map::new();
                for (key, item) in map {
                    let (item, count) = with_unknown_everywhere(item);
                    added += count;
                    out.insert(key.clone(), item);
                }
                out.insert(
                    "added_later".into(),
                    json!({"by": "a newer store", "items": [1, {"deeper": true}]}),
                );
                (Value::Object(out), added)
            }
            Value::Array(items) => {
                let mut added = 0;
                let items = items
                    .iter()
                    .map(|item| {
                        let (item, count) = with_unknown_everywhere(item);
                        added += count;
                        item
                    })
                    .collect();
                (Value::Array(items), added)
            }
            other => (other.clone(), 0),
        }
    }

    fn reply(value: &Value) -> Exit {
        Exit::Code {
            code: 0,
            stdout: value.to_string().into_bytes(),
            stderr: String::new(),
        }
    }

    /// A read's decode and view of a reply, with and without the unknown fields.
    fn same_read(read: TeamRead, name: &str, view: impl Fn(&Value) -> Value) {
        let original = fixture(name);
        let (later, added) = with_unknown_everywhere(&original);
        assert!(added > 3, "{name}: {added} objects");
        let first = decode(&read, reply(&original)).unwrap();
        let second =
            decode(&read, reply(&later)).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(view(&second), view(&first), "{name}");
    }

    #[test]
    fn the_catalog_ignores_unknown_fields_at_every_level() {
        let original = fixture("catalog");
        let (later, added) = with_unknown_everywhere(&original);
        assert!(added > 30, "catalog, its templates, roster seats, settings, delegations, roles, pipelines and stages: {added}");
        let read =
            form_catalog(&later).expect("a catalog with fields this version does not know is read");
        assert_eq!(read, form_catalog(&original).unwrap());
        assert!(read["catalog"]["templates"][0]["digest"].is_string());
    }

    #[test]
    fn the_list_ignores_unknown_fields_at_every_level() {
        same_read(TeamRead::List { after: None }, "list", |value| {
            serde_json::to_value(list(value)).unwrap()
        });
    }

    #[test]
    fn the_status_and_its_members_ignore_unknown_fields_at_every_level() {
        let queue = fixture("queue");
        same_read(TeamRead::Status { team: TEAM.into() }, "status", |value| {
            serde_json::to_value(team(value, &queue)).unwrap()
        });
    }

    #[test]
    fn the_queue_and_its_rows_ignore_unknown_fields_at_every_level() {
        let status = fixture("status");
        same_read(
            TeamRead::Queue {
                team: TEAM.into(),
                after: None,
            },
            "queue",
            |value| serde_json::to_value(team(&status, value)).unwrap(),
        );
    }

    #[test]
    fn the_waiting_cards_ignore_unknown_fields_at_every_level() {
        same_read(TeamRead::Waiting { after: None }, "waiting", |value| {
            serde_json::to_value(waiting(value)).unwrap()
        });
        // The team cards reply's rows, read as the waiting view reads its cards.
        let as_waiting = |cards: &Value| {
            let rows: Vec<Value> = cards["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    let mut row = row.clone();
                    row["team"] = json!(TEAM);
                    row
                })
                .collect();
            serde_json::to_value(waiting(
                &json!({"ok": true, "actor": {"kind": "person"}, "rows": rows, "next": null}),
            ))
            .unwrap()
        };
        let cards = fixture("cards");
        let (later, added) = with_unknown_everywhere(&cards);
        assert!(added > 3);
        assert_eq!(as_waiting(&later), as_waiting(&cards));
    }

    #[test]
    fn a_problem_ignores_unknown_fields_and_its_detail() {
        for name in ["storage-failed", "template-changed"] {
            let original = fixture(name);
            let (mut later, _) = with_unknown_everywhere(&original);
            later["error"]["detail"] = json!({"retry_after_ms": 50, "refs": ["x"]});
            let parse = |value: &Value| crate::teams::refusal::parse(value.to_string().as_bytes());
            assert_eq!(parse(&later), parse(&original), "{name}");
            assert!(parse(&later).is_some(), "{name}");
        }
    }

    #[test]
    fn the_reader_still_refuses_what_is_wrong_and_the_types_stay_strict() {
        use zerus_team_plan::catalog::Catalog;
        let catalog = fixture("catalog")["catalog"].clone();
        let (later, _) = with_unknown_everywhere(&catalog);
        assert!(
            serde_json::from_value::<Catalog>(later.clone()).is_err(),
            "the strict type still refuses unknown fields"
        );
        assert!(read::<Catalog>(later).is_ok());
        let mut missing = catalog.clone();
        missing["templates"][0]
            .as_object_mut()
            .unwrap()
            .remove("roster");
        assert!(
            read::<Catalog>(missing).is_err(),
            "a missing field is refused"
        );
        let mut wrong = catalog.clone();
        wrong["templates"][0]["roster"] = json!("two");
        assert!(read::<Catalog>(wrong).is_err(), "a wrong type is refused");
        let mut variant = catalog;
        variant["pipelines"][0]["stages"][0]["kind"] = json!("deploy");
        assert!(
            read::<Catalog>(variant).is_err(),
            "an unknown variant is refused"
        );
    }
}
