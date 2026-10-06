//! The generated `phasekit-data` crate (ARCHITECTURE.md §8 steps 6-7; PLAN.md M2.5): one blob per fluid, the sorted
//! name/alias/CAS/InChIKey index with each fluid's ECS references, and one Cargo feature per fluid.
//!
//! - Keys are ASCII case-folded (map 09 §4.5): CoolProp lists "water" and "WATER" as separate aliases; here they
//!   collapse into one key, and a key two fluids share is refused. v8.0.0 has 556 distinct keys and no collision.
//! - A fluid's feature is `fluid-<slug>`: its name lowercased, each run of other characters a `-`, the ends trimmed
//!   (`R1234ze(E)` → `fluid-r1234ze-e`; PLAN.md §2.5). The feature enables its ECS reference fluids' features, so a
//!   subset build never lacks a reference (map 05 §5); the reference graph is refused if it has a cycle (map 05 R7).
//! - Everything is sorted and written deterministically, so `gates datagen` compares the committed files byte for
//!   byte with a fresh generation (map 09 R13).

use super::Source;

/// The generated crate, relative to the repository root.
pub const DATA_CRATE: &str = "crates/phasekit-data";

/// The fluids of the `core` feature: the TDD set of map 09 §9.
pub const CORE: [&str; 5] = ["Water", "Nitrogen", "CarbonDioxide", "R134a", "n-Propane"];

/// One fluid of the index.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Aliases, CAS and InChIKey in CoolProp's order, each case-folded key once and none equal to the name's.
    pub aliases: Vec<String>,
    /// Canonical names of the ECS reference fluids of its transport models, sorted.
    pub requires: Vec<String>,
    pub feature: String,
    pub blob: Vec<u8>,
}

impl Entry {
    /// The blob's file name under `blobs/`.
    pub fn file(&self) -> String {
        format!("{}.bin", self.feature.trim_start_matches("fluid-"))
    }

    /// Every case-folded key of this fluid, the name's first.
    pub fn keys(&self) -> Vec<String> {
        core::iter::once(&self.name).chain(&self.aliases).map(|k| k.to_ascii_lowercase()).collect()
    }
}

/// `fluid-<slug>` (PLAN.md §2.5).
pub fn feature_name(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    format!("fluid-{}", slug.trim_end_matches('-'))
}

/// The ECS reference fluids a file's transport names (`TRANSPORT.{viscosity,conductivity}.reference_fluid`),
/// as written.
fn references(source: &Source) -> Vec<String> {
    let transport = source.fluid.transport.as_ref();
    let named = ["viscosity", "conductivity"].into_iter().filter_map(|model| {
        let reference = transport?.get(model)?.get("reference_fluid")?;
        reference.as_str().map(str::to_string)
    });
    named.collect()
}

/// The index of `fluids` (parsed file, blob), sorted by name: keys collapsed and checked, references resolved to
/// canonical names and proved acyclic, features named.
pub fn index(fluids: &[(&Source, Vec<u8>)]) -> Result<Vec<Entry>, String> {
    let mut entries: Vec<Entry> = Vec::new();
    for (source, blob) in fluids {
        let info = &source.fluid.info;
        let mut keys = vec![info.name.to_ascii_lowercase()];
        let mut aliases = Vec::new();
        for id in info.aliases.iter().chain([&info.cas]).chain(&info.inchi_key) {
            if !keys.contains(&id.to_ascii_lowercase()) {
                keys.push(id.to_ascii_lowercase());
                aliases.push(id.clone());
            }
        }
        let entry = Entry {
            name: info.name.clone(),
            aliases,
            requires: references(source),
            feature: feature_name(&info.name),
            blob: blob.clone(),
        };
        entries.push(entry);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let mut owner: Vec<(String, &str)> =
        entries.iter().flat_map(|e| e.keys().into_iter().map(move |k| (k, e.name.as_str()))).collect();
    owner.sort();
    if let Some(w) = owner.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(format!("key {:?} names both {} and {}", w[0].0, w[0].1, w[1].1));
    }
    let mut features: Vec<&str> = entries.iter().map(|e| e.feature.as_str()).collect();
    features.sort_unstable();
    if let Some(w) = features.windows(2).find(|w| w[0] == w[1]) {
        return Err(format!("two fluids share the feature {}", w[0]));
    }

    let canonical = |key: &str| -> Option<String> {
        let i = owner.binary_search_by(|(k, _)| k.as_str().cmp(&key.to_ascii_lowercase())).ok()?;
        Some(owner[i].1.to_string())
    };
    let resolved: Vec<Vec<String>> = entries
        .iter()
        .map(|e| {
            let mut requires = e
                .requires
                .iter()
                .map(|r| canonical(r).ok_or_else(|| format!("{}: unknown ECS reference fluid {r:?}", e.name)))
                .collect::<Result<Vec<_>, _>>()?;
            requires.sort();
            requires.dedup();
            Ok(requires)
        })
        .collect::<Result<_, String>>()?;
    for (entry, requires) in entries.iter_mut().zip(resolved) {
        entry.requires = requires;
    }
    check_acyclic(&entries)?;
    let missing: Vec<&str> = CORE.iter().copied().filter(|c| !entries.iter().any(|e| e.name == *c)).collect();
    if !missing.is_empty() {
        return Err(format!("core fluids missing from the data: {missing:?}"));
    }
    Ok(entries)
}

/// Refuses a reference cycle (a fluid reaching itself through ECS references; map 05 R7).
fn check_acyclic(entries: &[Entry]) -> Result<(), String> {
    for start in entries {
        let mut path = vec![start.name.as_str()];
        let mut frontier: Vec<&str> = start.requires.iter().map(String::as_str).collect();
        let mut seen: Vec<&str> = Vec::new();
        while let Some(name) = frontier.pop() {
            if name == start.name {
                path.push(name);
                return Err(format!("ECS reference cycle through {}", path.join(" -> ")));
            }
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            path.push(name);
            if let Some(next) = entries.iter().find(|e| e.name == name) {
                frontier.extend(next.requires.iter().map(String::as_str));
            }
        }
    }
    Ok(())
}

fn quoted(xs: &[String]) -> String {
    xs.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>().join(", ")
}

/// `crates/phasekit-data/src/generated.rs`.
pub fn generated_rs(entries: &[Entry]) -> String {
    let mut out = String::from(
        "// @generated by `cargo xtask datagen` (PLAN.md M2.5) from CoolProp v8.0.0's fluid files; do not edit.\n\n\
         use crate::FluidEntry;\n\n\
         /// Identifier of the dataset these blobs were generated from (recorded in fixtures and `Source`).\n\
         pub const DATASET: &str = \"coolprop-8.0.0+ae81610e\";\n\n\
         /// The embedded index: every CoolProp v8.0.0 fluid, sorted by canonical name.\n\
         pub static FLUIDS: &[FluidEntry] = &[\n",
    );
    for e in entries {
        out.push_str(&format!(
            "    FluidEntry {{\n        name: {:?},\n        aliases: &[{}],\n        requires: &[{}],\n        \
             feature: {:?},\n        blob: blob!({:?}, {:?}),\n    }},\n",
            e.name,
            quoted(&e.aliases),
            quoted(&e.requires),
            e.feature,
            e.feature,
            e.file()
        ));
    }
    out.push_str("];\n");
    out
}

/// A TOML array of strings, one per line when there are several.
fn toml_array(xs: &[String]) -> String {
    match xs {
        [] => "[]".into(),
        [x] => format!("[{x:?}]"),
        _ => format!("[\n{}]", xs.iter().map(|x| format!("    {x:?},\n")).collect::<String>()),
    }
}

/// `crates/phasekit-data/Cargo.toml`, features included.
pub fn cargo_toml(entries: &[Entry]) -> String {
    let mut core: Vec<String> = CORE.iter().map(|c| feature_name(c)).collect();
    core.sort();
    let all: Vec<String> = entries.iter().map(|e| e.feature.clone()).collect();
    let mut out = String::from(
        "# @generated by `cargo xtask datagen` (PLAN.md M2.5); do not edit.\n\
         [package]\n\
         name = \"phasekit-data\"\n\
         description = \"Generated CoolProp v8.0.0 fluid blobs and name index (data only, no logic)\"\n\
         version.workspace = true\n\
         edition.workspace = true\n\
         rust-version.workspace = true\n\
         license.workspace = true\n\
         publish.workspace = true\n\n\
         # D7: one feature per fluid. A fluid's feature enables the features of its ECS transport reference fluids, so\n\
         # a subset build can never miss a reference (map 05 §5). `core` is the TDD set of map 09 §9.\n\
         [features]\n\
         default = [\"all\"]\n",
    );
    out.push_str(&format!("all = {}\n", toml_array(&all)));
    out.push_str(&format!("core = {}\n", toml_array(&core)));
    for e in entries {
        let refs: Vec<String> = e
            .requires
            .iter()
            .filter_map(|r| entries.iter().find(|x| x.name == *r))
            .map(|x| x.feature.clone())
            .collect();
        out.push_str(&format!("{} = {}\n", e.feature, toml_array(&refs)));
    }
    out.push_str("\n[lints]\nworkspace = true\n");
    out
}

/// Every generated file: (path relative to the repository root, bytes), sorted by path.
pub fn outputs(entries: &[Entry]) -> Vec<(String, Vec<u8>)> {
    let mut files = vec![
        (format!("{DATA_CRATE}/Cargo.toml"), cargo_toml(entries).into_bytes()),
        (format!("{DATA_CRATE}/src/generated.rs"), generated_rs(entries).into_bytes()),
    ];
    files.extend(entries.iter().map(|e| (format!("{DATA_CRATE}/blobs/{}", e.file()), e.blob.clone())));
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repo;

    fn entries() -> Vec<Entry> {
        super::super::generate(&Repo::locate()).unwrap().1
    }

    /// PLAN.md §2.5: one feature per fluid, `fluid-<slug>`, all distinct.
    #[test]
    fn feature_names_are_unique() {
        let cases =
            [("R1234ze(E)", "fluid-r1234ze-e"), ("n-Propane", "fluid-n-propane"), ("1-Butene", "fluid-1-butene")];
        for (name, feature) in cases {
            assert_eq!(feature_name(name), feature);
        }
        assert_eq!(feature_name("A..B--(C)"), "fluid-a-b-c");
        let entries = entries();
        let mut features: Vec<&str> = entries.iter().map(|e| e.feature.as_str()).collect();
        features.sort_unstable();
        features.dedup();
        assert_eq!(features.len(), 136);
    }

    /// Map 05 R7: ECS references resolve to canonical names (R14's "Nitrogen", R32's "Propane" → n-Propane), 19
    /// fluids have one, and the graph has no cycle; a cycle and an unknown reference are refused.
    #[test]
    fn ecs_reference_graph_is_acyclic() {
        let entries = entries();
        let with: Vec<(&str, &[String])> =
            entries.iter().filter(|e| !e.requires.is_empty()).map(|e| (e.name.as_str(), &e.requires[..])).collect();
        assert_eq!(with.len(), 19);
        assert!(with.contains(&("R32", &["n-Propane".to_string()][..])));
        assert!(with.contains(&("R14", &["Nitrogen".to_string()][..])));
        let referenced: Vec<&String> = with.iter().flat_map(|(_, r)| r.iter()).collect();
        assert!(referenced.iter().all(|r| ["n-Propane", "R134a", "Nitrogen"].contains(&r.as_str())));

        let mut cyclic = entries.clone();
        let r134a = cyclic.iter_mut().find(|e| e.name == "R134a").unwrap();
        r134a.requires = vec!["R143a".into()];
        assert_eq!(check_acyclic(&cyclic), Err("ECS reference cycle through R134a -> R143a -> R134a".to_string()));
        assert!(check_acyclic(&entries).is_ok());
    }

    /// A fluid's feature turns on its references' features (map 05 §5), and the generated Cargo.toml says so; the
    /// `core` feature is the TDD set (map 09 §9) and `all` is every fluid.
    #[test]
    fn fluid_feature_enables_its_references() {
        let entries = entries();
        let toml = cargo_toml(&entries);
        assert!(toml.contains("\nfluid-r143a = [\"fluid-r134a\"]\n"));
        assert!(toml.contains("\nfluid-r32 = [\"fluid-n-propane\"]\n"));
        assert!(toml.contains("\nfluid-water = []\n"));
        let core = "core = [\n    \"fluid-carbondioxide\",\n    \"fluid-n-propane\",\n    \"fluid-nitrogen\",\n    \"fluid-r134a\",\n    \"fluid-water\",\n]\n";
        assert!(toml.contains(core), "{toml}");
        assert_eq!(toml.matches("    \"fluid-").count(), 136 + 5);
        for e in &entries {
            let refs: Vec<String> = e.requires.iter().map(|r| feature_name(r)).collect();
            let line = format!("\n{} = {}\n", e.feature, toml_array(&refs));
            assert!(toml.contains(&line), "{line}");
        }
    }

    /// Two fluids claiming one key, or one feature, are refused (the index never resolves a name two ways), and so
    /// is data without the `core` feature's fluids.
    #[test]
    fn colliding_keys_and_features_are_refused() {
        let repo = Repo::locate();
        let sources = super::super::load(&repo).unwrap();
        let pick = |file: &str| sources.iter().find(|s| s.file == file).unwrap();
        let (water, ammonia) = (pick("Water.json"), pick("Ammonia.json"));
        let mut thief = ammonia.fluid.clone();
        thief.info.aliases.push("h2o".into());
        let thief = Source { fluid: thief, file: "Ammonia.json".into(), tree: ammonia.tree.clone(), waivers: vec![] };
        let err = index(&[(water, vec![]), (&thief, vec![])]).unwrap_err();
        assert_eq!(err, "key \"h2o\" names both Ammonia and Water");
        let mut twin = ammonia.fluid.clone();
        twin.info.name = "WATER!".into();
        twin.info.aliases.clear();
        (twin.info.cas, twin.info.inchi_key) = ("x".into(), None);
        let twin = Source { fluid: twin, file: "x.json".into(), tree: ammonia.tree.clone(), waivers: vec![] };
        assert_eq!(index(&[(water, vec![]), (&twin, vec![])]).unwrap_err(), "two fluids share the feature fluid-water");
        let core_missing =
            "core fluids missing from the data: [\"Nitrogen\", \"CarbonDioxide\", \"R134a\", \"n-Propane\"]";
        assert_eq!(index(&[(water, vec![])]).unwrap_err(), core_missing);
    }

    /// `gates datagen` (VERIFICATION.md §11.2): the committed files equal a fresh generation byte for byte; a changed
    /// byte, a missing output and a blob nothing generates are each reported.
    #[test]
    fn generated_files_are_compared_byte_for_byte() {
        let repo = Repo::locate();
        let mut outputs = outputs(&entries());
        assert_eq!(super::super::differences(&repo, &outputs), Ok(vec![]));
        outputs[0].1.push(b'\n');
        let extra = (format!("{DATA_CRATE}/blobs/zz-new.bin"), vec![1]);
        outputs.push(extra);
        let dropped = outputs.iter().position(|(p, _)| p.ends_with("/blobs/water.bin")).unwrap();
        outputs.remove(dropped);
        let found = super::super::differences(&repo, &outputs).unwrap();
        assert_eq!(
            found,
            [
                format!("{DATA_CRATE}/Cargo.toml differs from a fresh generation"),
                format!("{DATA_CRATE}/blobs/zz-new.bin is missing"),
                format!("{DATA_CRATE}/blobs/water.bin is not generated by datagen"),
            ]
        );
    }
}
