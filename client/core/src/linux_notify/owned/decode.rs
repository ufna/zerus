//! Streaming raw-member checks: never deserialize IDs through a coalescing map.
use super::*;
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use std::fmt;

#[derive(Clone, Copy)]
struct Mode(bool);
enum RawToken {
    Legacy(String),
    Owned(RawEntry),
}
struct RawEntry {
    owner: String,
    token: String,
    unknown: bool,
}
#[derive(Default)]
struct Raw {
    version: Option<u32>,
    bus_id: Option<String>,
    owner: Option<String>,
    tokens: Option<Vec<(String, RawToken)>>,
    uncertain: Option<Vec<RawToken>>,
    legacy: Option<Vec<String>>,
    unknown: bool,
}
fn once<'de, M: MapAccess<'de>, T: Deserialize<'de>>(
    map: &mut M,
    slot: &mut Option<T>,
    name: &'static str,
) -> Result<(), M::Error> {
    if slot.is_some() {
        return Err(de::Error::duplicate_field(name));
    }
    *slot = Some(map.next_value()?);
    Ok(())
}
impl<'de> DeserializeSeed<'de> for Mode {
    type Value = Raw;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Raw, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Mode {
    type Value = Raw;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("notification object")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Raw, M::Error> {
        let mut raw = Raw::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "version" => once(&mut map, &mut raw.version, "version")?,
                "bus_id" => once(&mut map, &mut raw.bus_id, "bus_id")?,
                "owner" => once(&mut map, &mut raw.owner, "owner")?,
                "tokens" => {
                    if raw.tokens.is_some() {
                        return Err(de::Error::duplicate_field("tokens"));
                    }
                    raw.tokens = Some(map.next_value_seed(TokenMap(self))?);
                }
                "uncertain" => {
                    if raw.uncertain.is_some() {
                        return Err(de::Error::duplicate_field("uncertain"));
                    }
                    raw.uncertain = Some(map.next_value_seed(TokenList(self))?);
                }
                "legacy_uncertain" if self.0 => {
                    if raw.legacy.is_some() {
                        return Err(de::Error::duplicate_field("legacy_uncertain"));
                    }
                    raw.legacy = Some(
                        map.next_value_seed(TokenList(Mode(false)))?
                            .into_iter()
                            .map(|token| match token {
                                RawToken::Legacy(s) => s,
                                _ => unreachable!(),
                            })
                            .collect(),
                    );
                }
                _ => {
                    raw.unknown = true;
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(raw)
    }
}
struct TokenMap(Mode);
impl<'de> DeserializeSeed<'de> for TokenMap {
    type Value = Vec<(String, RawToken)>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for TokenMap {
    type Value = Vec<(String, RawToken)>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded ID map")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut values = Vec::new();
        while let Some(id) = map.next_key::<String>()? {
            if values.len() == LIMIT {
                return Err(de::Error::custom("notification entry limit"));
            }
            values.push((id, map.next_value_seed(Token(self.0))?));
        }
        Ok(values)
    }
}
struct TokenList(Mode);
impl<'de> DeserializeSeed<'de> for TokenList {
    type Value = Vec<RawToken>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for TokenList {
    type Value = Vec<RawToken>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded token list")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Token(self.0))? {
            if values.len() == LIMIT {
                return Err(de::Error::custom("notification entry limit"));
            }
            values.push(value);
        }
        Ok(values)
    }
}
struct Token(Mode);
impl<'de> DeserializeSeed<'de> for Token {
    type Value = RawToken;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<RawToken, D::Error> {
        if self.0 .0 {
            d.deserialize_map(EntryVisitor).map(RawToken::Owned)
        } else {
            String::deserialize(d).map(RawToken::Legacy)
        }
    }
}
struct EntryVisitor;
impl<'de> Visitor<'de> for EntryVisitor {
    type Value = RawEntry;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("explicit owner and token")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<RawEntry, M::Error> {
        let (mut owner, mut token, mut unknown) = (None, None, false);
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "owner" => once(&mut map, &mut owner, "owner")?,
                "token" => once(&mut map, &mut token, "token")?,
                _ => {
                    unknown = true;
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(RawEntry {
            owner: owner.ok_or_else(|| de::Error::missing_field("owner"))?,
            token: token.ok_or_else(|| de::Error::missing_field("token"))?,
            unknown,
        })
    }
}
fn scoped(value: RawToken, unknown: &mut bool) -> Result<ScopedToken, Refused> {
    let RawToken::Owned(value) = value else {
        return Err(Refused::Malformed);
    };
    *unknown |= value.unknown;
    let owner = match value.owner.as_str() {
        "agent_attention" => Owner::AgentAttention,
        "software_update" => Owner::SoftwareUpdate,
        _ => return Err(Refused::InvalidIdentity),
    };
    ScopedToken::new(owner, value.token)
}
fn legacy(value: RawToken) -> Result<String, Refused> {
    let RawToken::Legacy(value) = value else {
        return Err(Refused::Malformed);
    };
    if !legacy_valid(&value) {
        return Err(Refused::InvalidIdentity);
    }
    Ok(value)
}
pub(super) fn read(bytes: &[u8], owned: bool) -> Result<OwnedSaved, Refused> {
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(Refused::TooLarge);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let raw = Mode(owned)
        .deserialize(&mut deserializer)
        .map_err(|_| Refused::Malformed)?;
    deserializer.end().map_err(|_| Refused::Malformed)?;
    if raw.version != Some(if owned { 2 } else { 1 }) {
        return Err(Refused::UnsupportedVersion);
    }
    let tokens = raw.tokens.ok_or(Refused::Malformed)?;
    let uncertain = match raw.uncertain {
        Some(values) => values,
        None if !owned => Vec::new(),
        _ => return Err(Refused::Malformed),
    };
    let legacy_values = match raw.legacy {
        Some(values) => values,
        None if !owned => Vec::new(),
        _ => return Err(Refused::Malformed),
    };
    let raw_entries = tokens.len() + uncertain.len() + legacy_values.len();
    if raw_entries > LIMIT {
        return Err(Refused::TooLarge);
    }
    if (owned && legacy_values.len() > LEGACY_LIMIT) || (!owned && raw_entries > LEGACY_LIMIT) {
        return Err(Refused::LegacyCapacity);
    }
    let owner = raw.owner.ok_or(Refused::Malformed)?;
    let bus_id = match raw.bus_id {
        Some(value) => value,
        None if !owned => String::new(),
        _ => return Err(Refused::Malformed),
    };
    let namespace = if owned {
        namespace(bus_id, owner)?
    } else {
        if !bus_id.is_empty()
            && (bus_id.len() != 32 || !bus_id.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(Refused::InvalidNamespace);
        }
        None // No old numeric ID retains action/removal authority.
    };
    let mut result = OwnedSaved {
        namespace,
        bindings: BTreeMap::new(),
        uncertain: BTreeSet::new(),
        legacy: BTreeSet::new(),
        has_unknown: raw.unknown,
        raw_entries,
    };
    for (text, token) in tokens {
        let id = text
            .parse::<u32>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or(Refused::InvalidIdentity)?;
        if owned {
            if id.to_string() != text {
                return Err(Refused::InvalidIdentity);
            }
            let token = scoped(token, &mut result.has_unknown)?;
            if result.bindings.insert(id, token).is_some() {
                return Err(Refused::DuplicateIdentity);
            }
        } else {
            result.legacy.insert(legacy(token)?);
        }
    }
    if owned && result.namespace.is_none() && !result.bindings.is_empty() {
        return Err(Refused::InvalidNamespace);
    }
    for token in uncertain {
        if owned {
            result
                .uncertain
                .insert(scoped(token, &mut result.has_unknown)?);
        } else {
            result.legacy.insert(legacy(token)?);
        }
    }
    for token in legacy_values {
        if !legacy_valid(&token) {
            return Err(Refused::InvalidIdentity);
        }
        result.legacy.insert(token);
    }
    Ok(result)
}
