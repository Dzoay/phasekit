//! CoolProp's superancillary freshness stamp (map 09 §3): FNV-1a-64 over a type-tagged walk of a JSON tree, in
//! lockstep with `dev/scripts/inject_superanc_check_points.py::eos_fnv1a_hex` and its C++ twin `TreeHasher`
//! (`src/Tests/CoolProp-Tests.cpp:3598-3830`). Tags: `n`, `f`, `t`; `i` + LE int64; `d` + LE IEEE bits; `s`, `a`, `o`
//! + LE u64 length; object keys walked in UTF-8 byte order. The walk sees literal kinds, so `1` and `1.0` differ.

use super::json::Json;

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

struct Fnv(u64);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(PRIME);
        }
    }

    fn tag(&mut self, tag: u8) {
        self.bytes(&[tag]);
    }

    fn len(&mut self, n: usize) {
        self.bytes(&(n as u64).to_le_bytes());
    }

    fn walk(&mut self, json: &Json) {
        match json {
            Json::Null => self.tag(b'n'),
            Json::Bool(b) => self.tag(if *b { b't' } else { b'f' }),
            Json::Int(i) => {
                self.tag(b'i');
                self.bytes(&i.to_le_bytes());
            }
            Json::Float(x) => {
                self.tag(b'd');
                self.bytes(&x.to_bits().to_le_bytes());
            }
            Json::Str(s) => {
                self.tag(b's');
                self.len(s.len());
                self.bytes(s.as_bytes());
            }
            Json::Array(items) => {
                self.tag(b'a');
                self.len(items.len());
                items.iter().for_each(|item| self.walk(item));
            }
            Json::Object(members) => {
                self.tag(b'o');
                self.len(members.len());
                let mut sorted: Vec<&(String, Json)> = members.iter().collect();
                sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                for (key, value) in sorted {
                    self.len(key.len());
                    self.bytes(key.as_bytes());
                    self.walk(value);
                }
            }
        }
    }
}

/// The stamp of a tree (duplicate keys already resolved), as 16 lower-case hex digits.
pub fn fnv1a_hex(json: &Json) -> String {
    let mut hash = Fnv(OFFSET);
    hash.walk(json);
    format!("{:016x}", hash.0)
}

/// The stamp CoolProp compares with `EOS[0].SUPERANCILLARY.source_eos_hash`: `EOS[0]` without that subtree.
pub fn eos_stamp(fluid: &Json) -> Option<String> {
    let Json::Object(members) = fluid.values_at("EOS[0]").pop()? else { return None };
    let stripped = members.iter().filter(|(key, _)| key != "SUPERANCILLARY").cloned().collect();
    Some(fnv1a_hex(&Json::Object(stripped)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CoolProp's self-test (`CoolProp-Tests.cpp:3775-3789`): one fixture with every type of the contract. Its
    /// `alphar` is a one-element array (nlohmann reads the C++ initializer list that way; the object form hashes to
    /// 94f656eeb71f75c8).
    #[test]
    fn fnv_self_test_is_8e75626511d00b5c() {
        let fixture = Json::parse(
            r#"{"alphar": [{"d": [1, 2, 3], "n": [-0.5, 1.25e-10, 3.14159265358979]}], "empty_array": [],
                "empty_string": "", "flag_false": false, "flag_true": true, "gas_constant": 8.3144598,
                "nested": {"deep": {"deeper": null}}, "zero_float": 0.0, "zero_int": 0}"#,
        )
        .unwrap();
        assert_eq!(fnv1a_hex(&fixture), "8e75626511d00b5c");
        let Json::Object(mut members) = fixture else { panic!("an object") };
        members.reverse(); // key order in the file does not matter: the walk sorts
        assert_eq!(fnv1a_hex(&Json::Object(members.clone())), "8e75626511d00b5c");
        members[0].1 = Json::Float(0.0); // zero_int: 0 → 0.0
        assert_ne!(fnv1a_hex(&Json::Object(members)), "8e75626511d00b5c");
    }
}
