//! A literal-kind-preserving JSON tree (ARCHITECTURE.md §8 step 1). Integers and floats stay apart, so the FNV-1a
//! stamp can tell `1` from `1.0` (map 09 §3) and an integral float exponent is visible as one (map 02 §3.1); objects
//! keep every member in file order, duplicates included, so a duplicate key is found instead of silently replaced
//! (map 09 R19).

use core::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// One JSON value as written.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// An integer literal (no `.`, `e` or `E`).
    Int(i64),
    /// Any other number literal, parsed with correct rounding (serde_json's `float_roundtrip`).
    Float(f64),
    Str(String),
    Array(Vec<Json>),
    /// Members in file order, duplicates included.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Parses a JSON document.
    pub fn parse(text: &str) -> Result<Json, String> {
        if let Some(at) = negative_zero_integer(text) {
            return Err(format!("the integer literal -0 at byte {at} has no int64 value (CoolProp reads 0)"));
        }
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    /// Paths of keys that occur more than once in one object (`EOS[0].SUPERANCILLARY.source_eos_hash`).
    pub fn duplicate_keys(&self) -> Vec<String> {
        let mut found = Vec::new();
        self.find_duplicates("", &mut found);
        found
    }

    fn find_duplicates(&self, path: &str, found: &mut Vec<String>) {
        match self {
            Json::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    item.find_duplicates(&format!("{path}[{i}]"), found);
                }
            }
            Json::Object(members) => {
                for (i, (key, value)) in members.iter().enumerate() {
                    let path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                    if members[..i].iter().any(|(k, _)| k == key) && !found.contains(&path) {
                        found.push(path.clone());
                    }
                    value.find_duplicates(&path, found);
                }
            }
            _ => {}
        }
    }

    /// Every value stored under the key at `path`, in file order.
    pub fn values_at(&self, path: &str) -> Vec<&Json> {
        let mut node = vec![self];
        for step in path.split('.') {
            let (key, index) = match step.split_once('[') {
                Some((key, rest)) => (key, rest.trim_end_matches(']').parse::<usize>().ok()),
                None => (step, None),
            };
            node = node
                .into_iter()
                .flat_map(|n| match n {
                    Json::Object(members) => members.iter().filter(|(k, _)| k == key).map(|(_, v)| v).collect(),
                    _ => Vec::new(),
                })
                .filter_map(|v| match (index, v) {
                    (None, v) => Some(v),
                    (Some(i), Json::Array(items)) => items.get(i),
                    (Some(_), _) => None,
                })
                .collect();
        }
        node
    }

    /// The tree with each duplicate key reduced to its last value, as Python's `json` and nlohmann's parser (the
    /// reader of the CoolProp generator and library) both keep it.
    pub fn deduplicated(self) -> Json {
        match self {
            Json::Array(items) => Json::Array(items.into_iter().map(Json::deduplicated).collect()),
            Json::Object(members) => {
                let mut kept: Vec<(String, Json)> = Vec::with_capacity(members.len());
                for (key, value) in members {
                    let value = value.deduplicated();
                    match kept.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = value,
                        None => kept.push((key, value)),
                    }
                }
                Json::Object(kept)
            }
            other => other,
        }
    }

    /// The same document as a `serde_json::Value` (numbers keep their literal kind), for the typed mirror.
    pub fn to_value(&self) -> serde_json::Value {
        use serde_json::Value;
        match self {
            Json::Null => Value::Null,
            Json::Bool(b) => Value::Bool(*b),
            Json::Int(i) => Value::from(*i),
            Json::Float(x) => serde_json::Number::from_f64(*x).map_or(Value::Null, Value::Number),
            Json::Str(s) => Value::String(s.clone()),
            Json::Array(items) => Value::Array(items.iter().map(Json::to_value).collect()),
            Json::Object(members) => Value::Object(members.iter().map(|(k, v)| (k.clone(), v.to_value())).collect()),
        }
    }
}

/// The byte offset of an integer literal `-0` outside strings. serde_json reads it as the float −0.0 (int64 has no
/// negative zero) while CoolProp's readers read the integer 0, and the two hash differently; v8.0.0 has none (its one
/// negative zero, in Ammonia, is written `-0.0`), so the reader refuses the literal instead of guessing.
fn negative_zero_integer(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut in_string, mut escaped) = (false, false);
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            // An escaped character never ends the string; `\\` and `"` matter only unescaped.
            (in_string, escaped) = match (escaped, b) {
                (false, b'\\') => (true, true),
                (false, b'"') => (false, false),
                _ => (true, false),
            };
        } else if b == b'"' {
            in_string = true;
        } else if b == b'-'
            && bytes.get(i + 1) == Some(&b'0')
            && !matches!(bytes.get(i + 2), Some(b'0'..=b'9' | b'.' | b'e' | b'E'))
            && !matches!(i.checked_sub(1).and_then(|j| bytes.get(j)), Some(b'e' | b'E'))
        {
            return Some(i);
        }
    }
    None
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Json, D::Error> {
        deserializer.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_unit<E>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E>(self, b: bool) -> Result<Json, E> {
        Ok(Json::Bool(b))
    }

    fn visit_i64<E>(self, i: i64) -> Result<Json, E> {
        Ok(Json::Int(i))
    }

    /// CoolProp hashes integers as int64 (`inject_superanc_check_points.py`), so a larger one cannot be stamped.
    fn visit_u64<E: de::Error>(self, u: u64) -> Result<Json, E> {
        i64::try_from(u).map(Json::Int).map_err(|_| E::custom(format!("integer {u} does not fit in int64")))
    }

    fn visit_f64<E>(self, x: f64) -> Result<Json, E> {
        Ok(Json::Float(x))
    }

    fn visit_str<E>(self, s: &str) -> Result<Json, E> {
        Ok(Json::Str(s.to_string()))
    }

    fn visit_string<E>(self, s: String) -> Result<Json, E> {
        Ok(Json::Str(s))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Json::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut members = Vec::new();
        while let Some(member) = map.next_entry()? {
            members.push(member);
        }
        Ok(Json::Object(members))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Literal kinds survive the parse, and floats round correctly (`float_roundtrip`): 0.1 + 0.2's neighbours parse
    /// to their own doubles, not to a neighbour.
    #[test]
    fn literal_kinds_and_bits_are_kept() {
        let tree = Json::parse(r#"{"a": [1, 1.0, -0.0, 1e3, 0.30000000000000004, 0.3, null, true, "x"]}"#).unwrap();
        let Json::Object(members) = tree else { panic!("not an object") };
        let want = Json::Array(vec![
            Json::Int(1),
            Json::Float(1.0),
            Json::Float(-0.0),
            Json::Float(1000.0),
            Json::Float(0.1 + 0.2),
            Json::Float(0.3),
            Json::Null,
            Json::Bool(true),
            Json::Str("x".into()),
        ]);
        assert_eq!(members, vec![("a".to_string(), want)]);
        assert!(Json::parse("18446744073709551615").is_err(), "beyond int64");
        assert!(matches!(&members[0].1, Json::Array(a) if matches!(a[2], Json::Float(z) if z.is_sign_negative())));
    }

    /// A deserializer that offers something JSON cannot hold gets an error that says what was expected.
    #[test]
    fn non_json_input_says_what_was_expected() {
        let bytes = serde::de::value::BytesDeserializer::<serde::de::value::Error>::new(b"x");
        let err = Json::deserialize(bytes).unwrap_err().to_string();
        assert_eq!(err, "invalid type: byte array, expected any JSON value");
    }

    /// `-0` has no int64 value: refused, while `-0.0`, `-0e1`, `1e-0`, `-10` and `-0` inside a string are not.
    #[test]
    fn integer_negative_zero_is_refused() {
        assert_eq!(
            Json::parse("[1, -0]").unwrap_err(),
            "the integer literal -0 at byte 4 has no int64 value (CoolProp reads 0)"
        );
        assert!(Json::parse("{\"x\": -0}").is_err());
        for fine in ["[-0.0, -0e1, 1e-0, -10]", r#"{"k": "a -0 b", "q": "\\\"-0"}"#] {
            assert!(Json::parse(fine).is_ok(), "{fine}");
        }
    }

    /// Duplicates are reported with their path and resolved to the last value.
    #[test]
    fn duplicates_are_found_and_the_last_value_kept() {
        let tree = Json::parse(r#"{"E": [{"S": {"h": "1", "k": 2, "h": "3"}}], "h": 4}"#).unwrap();
        assert_eq!(tree.duplicate_keys(), vec!["E[0].S.h".to_string()]);
        assert_eq!(tree.values_at("E[0].S.h"), vec![&Json::Str("1".into()), &Json::Str("3".into())]);
        let tree = tree.deduplicated();
        assert!(tree.duplicate_keys().is_empty());
        assert_eq!(tree.values_at("E[0].S.h"), vec![&Json::Str("3".into())]);
        assert_eq!(tree.to_value()["E"][0]["S"], serde_json::json!({"h": "3", "k": 2}));
    }
}
