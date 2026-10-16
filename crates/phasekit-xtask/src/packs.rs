//! `cargo xtask packs <dir>` (PLAN.md M21.4): one pack per fluid of `phasekit-data`, which a page fetches and adds over
//! the browser package's embedded fluids with `withPack` (E6), and `index.json`, which lists them.
//!
//! A fluid's pack (`phasekit_core::internal::pack`) holds its names (canonical first, then CoolProp's aliases, CAS
//! number and InChIKey), the canonical names of the fluids it requires (its ECS transport references, map 05 §5) and its
//! v1 blob. It names its requirements without carrying them: a page adds each requirement's own pack first, unless its
//! registry already serves that fluid. `index.json` gives each fluid's name, its other aliases, its CAS number, its
//! requirements, and its pack's file and bytes. `scripts/site/build.sh` writes both into the page; they are never
//! committed.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use phasekit_data::{DATASET, FLUIDS, FluidEntry};
use serde_json::{Value, json};

use crate::repo::Repo;

/// One fluid's pack.
pub struct Packed {
    /// The fluid, as `phasekit-data` indexes it.
    pub entry: &'static FluidEntry,
    /// Its file: the fluid's feature without `fluid-`, then `.pack` (`r1234ze-e.pack`).
    pub file: String,
    /// The pack.
    pub bytes: Vec<u8>,
}

/// Every fluid of `phasekit-data`, a pack each, in its index's order (by canonical name).
pub fn packs() -> Vec<Packed> {
    let pack = |entry: &FluidEntry| {
        let names = core::iter::once(entry.name).chain(entry.aliases.iter().copied()).map(String::from).collect();
        let requires = entry.requires.iter().copied().map(String::from).collect();
        phasekit_core::internal::pack(&[(names, requires, Arc::from(entry.blob))])
    };
    let file = |entry: &FluidEntry| format!("{}.pack", entry.feature.trim_start_matches("fluid-"));
    FLUIDS.iter().map(|entry| Packed { entry, file: file(entry), bytes: pack(entry) }).collect()
}

/// The CAS registry number among `aliases`, if CoolProp lists one.
pub fn cas<'a>(aliases: &[&'a str]) -> Option<&'a str> {
    aliases.iter().copied().find(|a| is_cas(a))
}

/// Whether `s` is a CAS registry number: three groups of digits, of 2 to 7, 2 and 1, joined by hyphens, the last the
/// check digit.
fn is_cas(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    let [first, second, check] = groups[..] else { return false };
    let digits = |g: &str, lengths: core::ops::RangeInclusive<usize>| {
        lengths.contains(&g.len()) && g.bytes().all(|b| b.is_ascii_digit())
    };
    if !(digits(first, 2..=7) && digits(second, 2..=2) && digits(check, 1..=1)) {
        return false;
    }
    // The check digit is the sum of the other digits, each times its place counted from the right, modulo 10.
    let sum: u32 =
        first.bytes().chain(second.bytes()).rev().zip(1..).map(|(d, place)| u32::from(d - b'0') * place).sum();
    check.bytes().all(|d| u32::from(d - b'0') == sum % 10)
}

/// `index.json`: the dataset the blobs were generated from and, for each fluid, its canonical name, its other aliases
/// (CoolProp's, InChIKey included), its CAS number (null where CoolProp lists none), the canonical names it requires,
/// and its pack's file and bytes.
pub fn index(packs: &[Packed]) -> Value {
    let fluids: Vec<Value> = packs
        .iter()
        .map(|p| {
            let cas = cas(p.entry.aliases);
            let aliases: Vec<&str> = p.entry.aliases.iter().copied().filter(|a| Some(*a) != cas).collect();
            json!({
                "name": p.entry.name,
                "aliases": aliases,
                "cas": cas,
                "requires": p.entry.requires,
                "pack": p.file,
                "bytes": p.bytes.len(),
            })
        })
        .collect();
    json!({ "dataset": DATASET, "fluids": fluids })
}

/// Writes every pack and `index.json` into `<dir>`, and prints their count, total and largest sizes.
pub fn main(args: &[String]) -> ExitCode {
    let [dir] = args else {
        eprintln!("usage: cargo xtask packs <dir>");
        return ExitCode::FAILURE;
    };
    let (repo, dir, packs) = (Repo::locate(), Path::new(dir), packs());
    let write = || -> Result<usize, String> {
        packs.iter().try_for_each(|p| repo.write_outside_bytes(&dir.join(&p.file), &p.bytes))?;
        let index = format!("{}\n", index(&packs));
        repo.write_outside(&dir.join("index.json"), &index)?;
        Ok(index.len())
    };
    match write() {
        Ok(index) => {
            let total: usize = packs.iter().map(|p| p.bytes.len()).sum();
            let largest = packs.iter().max_by_key(|p| p.bytes.len());
            let (name, bytes) = largest.map_or(("none", 0), |p| (p.entry.name, p.bytes.len()));
            println!(
                "packs: {} packs in {}, {total} bytes in all, the largest {name}'s {bytes}; index.json {index} bytes",
                packs.len(),
                dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("packs: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use phasekit_core::{
        DataSet, Density, Error, FlashOptions, Input, LoadError, Pack, Phase, Prop, Registry, Temperature,
    };

    use super::*;

    /// The pack `bytes` as a layer over `base`, under the browser package's data set.
    fn layer(base: &Registry, bytes: &[u8]) -> Result<Registry, Error> {
        let pack = Pack::new(Arc::from(bytes)).map_err(Error::Load)?;
        base.with_source(Box::new(pack), DataSet::Corrected)
    }

    /// The fluids of `phasekit-data`'s `core` list, which the browser package embeds (E7), but `except`, as one layer.
    fn embedded_but(except: &str) -> Registry {
        let manifest = Repo::locate().read("crates/phasekit-data/Cargo.toml").unwrap();
        let core = crate::gates::core_list(&manifest).unwrap();
        let fluids: Vec<_> = FLUIDS
            .iter()
            .filter(|f| core.iter().any(|c| c == f.feature) && f.name != except)
            .map(|f| {
                let names = core::iter::once(f.name).chain(f.aliases.iter().copied()).map(String::from).collect();
                (names, Vec::new(), Arc::from(f.blob))
            })
            .collect();
        layer(&Registry::empty(), &phasekit_core::internal::pack(&fluids)).unwrap()
    }

    /// PLAN.md M21.4: every pack decodes (`Pack::new`), and its layer over the browser package's five embedded fluids
    /// (over the other four, for one of them) resolves the fluid by its canonical name and by each alias, and builds it.
    #[test]
    fn every_pack_layers_over_the_five_embedded_fluids() {
        let packs = packs();
        assert_eq!(packs.len(), 136);
        let five = embedded_but("");
        assert_eq!(five.names().count(), 5);
        let mut failures = Vec::new();
        for p in &packs {
            let base =
                if five.canonical_name(p.entry.name).is_some() { embedded_but(p.entry.name) } else { five.clone() };
            let reg = match layer(&base, &p.bytes) {
                Ok(reg) => reg,
                Err(e) => {
                    failures.push(format!("{}: {e}", p.entry.name));
                    continue;
                }
            };
            for name in core::iter::once(p.entry.name).chain(p.entry.aliases.iter().copied()) {
                if reg.canonical_name(name) != Some(p.entry.name) {
                    failures.push(format!("{}: `{name}` resolves to {:?}", p.entry.name, reg.canonical_name(name)));
                }
            }
            if let Err(e) = reg.get(p.entry.name) {
                failures.push(format!("{}: {e}", p.entry.name));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// PLAN.md M21.4: a fluid whose transport needs a reference fluid (R12's ECS viscosity, on R134a) computes its
    /// viscosity from its packs alone, the reference's added first, with the embedded registry's bits; its pack alone
    /// is refused, naming the reference it lacks.
    #[test]
    fn a_reference_comes_from_its_own_pack() {
        let packs = packs();
        let bytes = |name: &str| &packs.iter().find(|p| p.entry.name == name).unwrap().bytes;
        let missing = LoadError::MissingReference { fluid: "R12".into(), reference: "R134a".into() };
        assert_eq!(layer(&Registry::empty(), bytes("R12")).unwrap_err(), Error::Load(missing));
        let reg = layer(&layer(&Registry::empty(), bytes("R134a")).unwrap(), bytes("R12")).unwrap();
        let viscosity = |reg: &Registry| {
            let fluid = reg.get("R12").unwrap();
            let input = Input::dt(Density::molar(100.0).unwrap(), Temperature::new(300.0).unwrap());
            let state = fluid.flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap();
            fluid.prop(&state, Prop::Viscosity).unwrap()
        };
        let want = viscosity(Registry::embedded().unwrap());
        assert!(want > 0.0, "{want}");
        assert_eq!(viscosity(&reg).to_bits(), want.to_bits());
        assert_eq!(reg.loaded().collect::<Vec<_>>(), ["R134a", "R12"]);
    }

    /// PLAN.md M21.4: `index.json` lists every pack in order, with its file and bytes, and each fluid's aliases, CAS
    /// number and requirements; files are distinct, and the aliases repeat no CAS number. 126 fluids have one: Air, the
    /// four `.PPF` blends and SES36 list none, and the ortho and para forms of hydrogen and deuterium their parent's with
    /// a suffix (`1333-74-0p`), which stays an alias.
    #[test]
    fn the_index_lists_every_pack() {
        let packs = packs();
        let index = index(&packs);
        assert_eq!(index["dataset"], DATASET);
        let fluids = index["fluids"].as_array().unwrap();
        assert_eq!(fluids.len(), packs.len());
        for (f, p) in fluids.iter().zip(&packs) {
            assert_eq!(
                (&f["name"], &f["pack"], &f["bytes"]),
                (&json!(p.entry.name), &json!(p.file), &json!(p.bytes.len()))
            );
            assert_eq!(f["requires"], json!(p.entry.requires));
            let cas = f["cas"].as_str();
            assert!(!f["aliases"].as_array().unwrap().iter().any(|a| a.as_str() == cas), "{f}");
        }
        let mut files: Vec<&str> = packs.iter().map(|p| p.file.as_str()).collect();
        files.dedup();
        assert_eq!(files.len(), 136);
        assert_eq!(fluids.iter().filter(|f| f["cas"].is_string()).count(), 126);
        let r32 = fluids.iter().find(|f| f["name"] == "R32").unwrap();
        let want = json!({
            "name": "R32",
            "aliases": ["RWRIWBAIICGTTQ-UHFFFAOYSA-N"],
            "cas": "75-10-5",
            "requires": ["n-Propane"],
            "pack": "r32.pack",
            "bytes": r32["bytes"],
        });
        assert_eq!(r32, &want);
    }

    /// A CAS number: groups of 2 to 7, 2 and 1 digits, the last the check digit, the others' weighted sum mod 10. Each
    /// rule is broken alone too, by a number whose check digit is right: `5-00-5`, `12345678-90-0`, `75-100-8`, `75-1-2`.
    #[test]
    fn cas_numbers_are_checked() {
        for good in ["75-10-5", "7732-18-5", "1234567-89-5", "50-00-0"] {
            assert!(is_cas(good), "{good}");
        }
        for bad in [
            "5-00-5",
            "12345678-90-0",
            "75-100-8",
            "75-1-2",
            "75-10-4",
            "7732-18-6",
            "5-00-0",
            "12345678-90-6",
            "75-100-5",
            "75-1-0",
            "75-10-55",
            "75-10",
            "75-10-5-5",
            "7a-10-5",
            "75-1a-5",
            "75-10-a",
            "RWRIWBAIICGTTQ-UHFFFAOYSA-N",
            "R1234ze(E)",
        ] {
            assert!(!is_cas(bad), "{bad}");
        }
        assert_eq!(cas(&["R-32", "75-10-5", "RWRIWBAIICGTTQ-UHFFFAOYSA-N"]), Some("75-10-5"));
        assert_eq!(cas(&["R729", "AIR.PPF"]), None);
    }
}
