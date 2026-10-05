//! `oracle.lock` (VERIFICATION.md §3.2): the oracle's pinned environment as `key value` lines, read here with
//! `include_str!` (no parser dependency) so tests can check fixture headers against it. Editing a line is an
//! oracle-pin move (VERIFICATION.md §6.5).

/// The committed lock.
pub const ORACLE_LOCK: &str = include_str!("../fixtures/oracle.lock");

/// A parsed lock: its `key value` lines in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleLock<'a> {
    entries: Vec<(&'a str, &'a str)>,
}

impl<'a> OracleLock<'a> {
    /// Parses `key value` lines; `#` lines are comments, blank lines are skipped, a key appears once.
    pub fn parse(text: &'a str) -> Result<Self, String> {
        let mut entries: Vec<(&'a str, &'a str)> = Vec::new();
        for line in text.lines().filter(|line| !line.is_empty() && !line.starts_with('#')) {
            let (key, value) = line.split_once(' ').map(|(k, v)| (k, v.trim())).unwrap_or((line, ""));
            if value.is_empty() {
                return Err(format!("oracle.lock: `{key}` has no value"));
            }
            if entries.iter().any(|(k, _)| *k == key) {
                return Err(format!("oracle.lock: `{key}` appears twice"));
            }
            entries.push((key, value));
        }
        Ok(OracleLock { entries })
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> Option<&'a str> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    /// The keys of `config_json`, a flat JSON object of booleans, numbers and strings, in order.
    pub fn config_keys(&self) -> Option<Vec<&'a str>> {
        object_keys(self.get("config_json")?)
    }
}

/// The keys of a flat JSON object (values are booleans, numbers or strings), or `None` if it is not one.
fn object_keys(json: &str) -> Option<Vec<&str>> {
    let mut rest = json.strip_prefix('{')?.strip_suffix('}')?;
    let mut keys = Vec::new();
    while !rest.is_empty() {
        let (key, after) = string(rest)?;
        let after = scalar(after.strip_prefix(':')?)?;
        keys.push(key);
        rest = match after.strip_prefix(',') {
            Some("") => return None, // a trailing comma
            Some(next) => next,
            None if after.is_empty() => after,
            None => return None,
        };
    }
    Some(keys)
}

/// A JSON string at the start of `text`: its contents (escapes left as they are) and what follows the closing quote.
fn string(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('"')?;
    let mut escaped = false;
    for (i, c) in inner.char_indices() {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => return Some((&inner[..i], &inner[i + 1..])),
            _ => {}
        }
    }
    None
}

/// A JSON string, boolean, null or number at the start of `text`; returns what follows it.
fn scalar(text: &str) -> Option<&str> {
    if text.starts_with('"') {
        return string(text).map(|(_, rest)| rest);
    }
    let (token, rest) = text.split_at(text.find(',').unwrap_or(text.len()));
    let number = !token.is_empty() && token.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b));
    (number || matches!(token, "true" | "false" | "null")).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_sha256(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }

    /// VERIFICATION.md §3.2: the lock pins the wheel, its library, the interpreter, the fluid files and the config.
    #[test]
    fn oracle_lock_records_the_wheel() {
        let lock = OracleLock::parse(ORACLE_LOCK).unwrap();
        assert_eq!((lock.get("package"), lock.get("version")), (Some("CoolProp"), Some("8.0.0")));
        assert_eq!(lock.get("git"), Some("ae81610e7d23efc57f9d051c8e70a4d66e87537f"));
        assert_eq!(lock.get("wheel_tag"), Some("cp312-abi3-manylinux_2_17_x86_64"));
        assert_eq!((lock.get("so_name"), lock.get("so_size")), (Some("CoolProp.abi3.so"), Some("9050856")));
        assert_eq!(lock.get("python"), Some("3.12"));
        assert!(lock.get("so_sha256").is_some_and(is_sha256) && lock.get("fluids_sha256").is_some_and(is_sha256));
        assert_eq!(lock.get("uv_command"), Some("uv run --no-project --python 3.12 --with CoolProp==8.0.0"));
        let keys = lock.config_keys().unwrap();
        assert_eq!(keys.len(), 38, "{keys:?}");
        assert_eq!(keys.first(), Some(&"ALLOW_SVDSBTL_IN_PROPSSI"));
        assert!(keys.contains(&"ENABLE_SUPERANCILLARIES") && keys.contains(&"NORMALIZE_GAS_CONSTANTS"));
    }

    #[test]
    fn lock_lines_and_config_keys_are_strict() {
        assert_eq!(OracleLock::parse("# c\n\nkey  a value\n").unwrap().get("key"), Some("a value"));
        assert!(OracleLock::parse("key 1\nkey 2\n").is_err(), "a repeated key");
        assert!(OracleLock::parse("lonely\n").is_err(), "a key without a value");
        let keys = object_keys(r#"{"A":",","B":"x\"y:z","C":-1.5e3,"D":true,"E":null}"#);
        assert_eq!(keys, Some(vec!["A", "B", "C", "D", "E"]));
        assert_eq!(object_keys("{}"), Some(vec![]));
        for bad in
            [r#"{"A":1"#, r#"["A"]"#, r#"{"A" 1}"#, r#"{"A":{"B":1}}"#, r#"{A:1}"#, r#"{"A":1,}"#, r#"{"A":"x""B":1}"#]
        {
            assert_eq!(object_keys(bad), None, "{bad}");
        }
    }
}
