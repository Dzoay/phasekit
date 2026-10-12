//! `TRANSPORT` → core's transport record (PLAN.md M8.1; map 05 U2; ARCHITECTURE.md §8 steps 2-3).
//!
//! CoolProp parses these blocks by hand (`FluidLibrary.h:459-1013`): it never reads some keys (`q_D`, map 05 R2),
//! checks lengths with `assert` only (R11), takes a list's first entry without checking that there is one (R5) and
//! parses forms it cannot evaluate (R9). Datagen reads them strictly, one JSON object at a time ([`Block`]):
//!
//! - every key is read, or is documentation: `_note` and `note`, and the `*_units` string of a key read, which must
//!   name the unit datagen expects (`every_transport_key_is_consumed_or_waived` proves that each key read reaches the
//!   record);
//! - a `type` or `hardcoded` name outside core's closed forms is refused, CoolProp's dead terms included (ROT-116);
//! - parallel lists have one length, and fixed-size arrays their size (R11);
//! - of a model list only the first entry is mapped, an empty list is refused, and [`skipped`] logs the alternates
//!   (ROT-109); a conductivity list is refused, as v8.0.0 parses none;
//! - each model keeps its own `BibTeX` source (D14); its DOI comes from CoolProp's bibliography
//!   ([`super::citations::attach`]);
//! - what CoolProp fills in for an omitted key, datagen fills the same way: the Olchowy-Sengers constants
//!   (`OlchowySengers::COOLPROP`). Lennard-Jones parameters stay absent when the block omits them; CoolProp estimates
//!   them by Chung's method, which M8.9 makes a typed choice.

use core::convert::Infallible;

use phasekit_core::internal::{
    BatschinskiHildebrand, Chung, Conductivity, ConductivityCritical, ConductivityDilute, ConductivityModel,
    ConductivityResidual, EcsConductivity, EcsViscosity, FrictionCubic, FrictionSecond, FrictionTheory,
    HardcodedConductivity, HardcodedCriticalConductivity, HardcodedDiluteConductivity, HardcodedDiluteViscosity,
    HardcodedHigherViscosity, HardcodedViscosity, Kappa, LennardJones, MAX_POW, OlchowySengers, RhoSrCs, Series,
    StagedConductivity, StagedViscosity, TransportRecord, Viscosity, ViscosityDilute, ViscosityHigher,
    ViscosityInitial, ViscosityModel,
};
use phasekit_core::{DataTerms, Source};
use serde_json::{Map, Value};

/// The unit each `*_units` key must name. `q_D_units` says m, but `q_D` is a wavenumber in 1/m: every value is the
/// inverse of a length near 1 nm (the default 2e9 is 1/(5e-10 m)), and CoolProp reads it so (`CoolPropFluid.h:188`).
const UNITS: [(&str, &str); 14] = [
    ("T_critical_units", "K"),
    ("T_reduce_units", "K"),
    ("T_reducing_units", "K"),
    ("acentric_units", "-"),
    ("dipole_moment_D_units", "Debye"),
    ("epsilon_over_k_units", "K"),
    ("kappa_units", "-"),
    ("molar_mass_units", "kg/mol"),
    ("q_D_units", "m"),
    ("rhomass_reducing_units", "kg/m^3"),
    ("rhomolar_critical_units", "mol/m^3"),
    ("rhomolar_reduce_units", "mol/m^3"),
    ("rhomolar_reducing_units", "mol/m^3"),
    ("sigma_eta_units", "m"),
];

/// Blocks whose lists are unequal, read under a logged waiver: (file, block). Each has one exponent more than it has
/// coefficients; CoolProp sums over the coefficients (`TransportRoutines.cpp:683, 1213`), so the extra exponent is never
/// read, and datagen drops it the same way. The three blends' dilute terms match R404A's two-term form plus a stray
/// exponent; whether EthylBenzene's source (Huber, NIST RP912) has a third ψ coefficient is open (M8.8).
const LENGTH_WAIVERS: [(&str, &str); 4] = [
    ("EthylBenzene.json", "TRANSPORT.viscosity.psi"),
    ("R407C.json", "TRANSPORT.conductivity.dilute"),
    ("R410A.json", "TRANSPORT.conductivity.dilute"),
    ("R507A.json", "TRANSPORT.conductivity.dilute"),
];

/// One JSON object read strictly: [`Block::finish`] refuses every key not read that is not documentation.
struct Block<'a> {
    file: &'a str,
    path: String,
    map: &'a Map<String, Value>,
    read: Vec<&'a str>,
}

impl<'a> Block<'a> {
    fn new(file: &'a str, path: String, v: &'a Value) -> Result<Self, String> {
        let map = v.as_object().ok_or_else(|| format!("{path} is not an object"))?;
        Ok(Block { file, path, map, read: Vec::new() })
    }

    fn err(&self, message: impl core::fmt::Display) -> String {
        format!("{}: {message}", self.path)
    }

    fn has(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    fn get(&mut self, key: &str) -> Result<&'a Value, String> {
        let (key, value) = self.map.get_key_value(key).ok_or_else(|| self.err(format!("no {key}")))?;
        self.read.push(key);
        Ok(value)
    }

    fn num(&mut self, key: &str) -> Result<f64, String> {
        self.get(key)?.as_f64().ok_or_else(|| self.err(format!("{key} is not a number")))
    }

    /// A number, or CoolProp's default when the block omits it.
    fn num_or(&mut self, key: &str, default: f64) -> Result<f64, String> {
        if self.has(key) { self.num(key) } else { Ok(default) }
    }

    fn text(&mut self, key: &str) -> Result<&'a str, String> {
        self.get(key)?.as_str().ok_or_else(|| self.err(format!("{key} is not a string")))
    }

    fn nums(&mut self, key: &str) -> Result<Vec<f64>, String> {
        let list = self.get(key)?.as_array().ok_or_else(|| self.err(format!("{key} is not a list")))?;
        list.iter().map(|x| x.as_f64().ok_or_else(|| self.err(format!("{key} holds {x}, not a number")))).collect()
    }

    /// A list of exactly `N` numbers (map 05 R11).
    fn fixed<const N: usize>(&mut self, key: &str) -> Result<[f64; N], String> {
        let xs = self.nums(key)?;
        let len = xs.len();
        <[f64; N]>::try_from(xs).map_err(|_| self.err(format!("{key} has {len} entries, not {N}")))
    }

    /// An integer exponent within `0..=MAX_POW`.
    fn exponent(&mut self, key: &str) -> Result<u8, String> {
        let n = self.get(key)?.as_u64().filter(|n| *n <= MAX_POW as u64).and_then(|n| u8::try_from(n).ok());
        n.ok_or_else(|| self.err(format!("{key} is not an integer in 0..={MAX_POW}")))
    }

    /// The lists `keys`, checked to have one length (map 05 R11).
    fn parallel<const N: usize>(&mut self, keys: [&str; N]) -> Result<[Vec<f64>; N], String> {
        let mut lists: [Vec<f64>; N] = core::array::from_fn(|_| Vec::new());
        for (list, key) in lists.iter_mut().zip(keys) {
            *list = self.nums(key)?;
        }
        if LENGTH_WAIVERS.contains(&(self.file, self.path.as_str())) {
            let n = lists[0].len();
            lists.iter_mut().skip(1).for_each(|list| list.truncate(n));
        }
        let lengths: Vec<(&str, usize)> = keys.iter().copied().zip(lists.iter().map(Vec::len)).collect();
        super::record::same_lengths(&lengths).map_err(|e| self.err(e))?;
        Ok(lists)
    }

    fn series(&mut self, keys: [&str; 2]) -> Result<Series, String> {
        let [a, t] = self.parallel(keys)?;
        Series::new(a, t).map_err(|e| self.err(e))
    }

    /// The object at `key`.
    fn block(&mut self, key: &str) -> Result<Block<'a>, String> {
        let path = format!("{}.{key}", self.path);
        Block::new(self.file, path, self.get(key)?)
    }

    /// Refuses a key not read, unless it is documentation: a note, or the unit of a key read.
    fn finish(self) -> Result<(), String> {
        for (key, value) in self.map {
            if self.read.contains(&key.as_str()) {
                continue;
            }
            let unit = UNITS.iter().find(|(k, _)| k == key).map(|(_, unit)| *unit);
            let of_a_key_read = key.strip_suffix("_units").is_some_and(|base| self.read.contains(&base));
            match (key.as_str(), unit) {
                ("_note" | "note", _) if value.is_string() => {}
                (_, Some(unit)) if of_a_key_read && value.as_str() == Some(unit) => {}
                (_, Some(unit)) if of_a_key_read => return Err(self.err(format!("{key} is {value}, not {unit:?}"))),
                _ => return Err(self.err(format!("unknown field {key:?}"))),
            }
        }
        Ok(())
    }
}

/// A stage's form: a `hardcoded` model of its slot, or a `type`.
enum Kind<'a, H> {
    Hardcoded(H),
    Type(&'a str),
}

fn kind<'a, H>(b: &mut Block<'a>, hardcoded: fn(&str) -> Option<H>) -> Result<Kind<'a, H>, String> {
    if b.has("hardcoded") {
        let name = b.text("hardcoded")?;
        return hardcoded(name).map(Kind::Hardcoded).ok_or_else(|| b.err(format!("unknown hardcoded model {name:?}")));
    }
    Ok(Kind::Type(b.text("type")?))
}

fn unknown<T>(b: &Block<'_>, kind: &str) -> Result<T, String> {
    Err(b.err(format!("unknown type {kind:?}")))
}

/// The record of a file's `TRANSPORT` block; a file without one has no models.
pub fn record(file: &str, transport: Option<&Value>) -> Result<TransportRecord, String> {
    let mut record = TransportRecord::default();
    let Some(transport) = transport else { return Ok(record) };
    let mut b = Block::new(file, "TRANSPORT".into(), transport)?;
    if b.has("viscosity") {
        record.viscosity = Some(viscosity(active(file, b.get("viscosity")?)?)?);
    }
    if b.has("conductivity") {
        let entry = b.get("conductivity")?;
        if entry.is_array() {
            return Err("TRANSPORT.conductivity is a list; v8.0.0 has one conductivity model per fluid".into());
        }
        record.conductivity = Some(conductivity(Block::new(file, "TRANSPORT.conductivity".into(), entry)?)?);
    }
    b.finish()?;
    Ok(record)
}

/// The viscosity block, or the first entry of a list (CoolProp's default; ROT-109).
fn active<'a>(file: &'a str, viscosity: &'a Value) -> Result<Block<'a>, String> {
    match viscosity {
        Value::Array(entries) => {
            let first = entries.first().ok_or("TRANSPORT.viscosity is an empty list")?;
            Block::new(file, "TRANSPORT.viscosity[0]".into(), first)
        }
        entry => Block::new(file, "TRANSPORT.viscosity".into(), entry),
    }
}

/// What datagen does not map from a file's `TRANSPORT`, one line each: the alternate entries of a model list
/// (ROT-109), and the list entries a length waiver drops.
pub fn skipped(source: &super::Source) -> Vec<String> {
    let file = &source.file;
    let entries = source.fluid.transport.as_ref().and_then(|t| t.get("viscosity")?.as_array());
    let alternates = entries.into_iter().flatten().enumerate().skip(1).map(|(i, entry)| {
        let key = entry.get("BibTeX").and_then(Value::as_str).unwrap_or("no BibTeX");
        format!("{file}: TRANSPORT.viscosity[{i}] ({key}) is an alternate, not mapped")
    });
    let waived = LENGTH_WAIVERS.iter().filter(|(f, _)| f == file).map(|(_, block)| {
        format!(
            "{file}: {block} lists more exponents than coefficients; the extra ones are dropped, as CoolProp drops them"
        )
    });
    alternates.chain(waived).collect()
}

fn source(b: &mut Block<'_>) -> Result<Source, String> {
    Ok(Source::new(b.text("BibTeX")?, None, DataTerms::Published))
}

fn viscosity(mut b: Block<'_>) -> Result<Viscosity, String> {
    let source = source(&mut b)?;
    let lennard_jones = match (b.has("sigma_eta"), b.has("epsilon_over_k")) {
        (true, true) => Some(LennardJones { sigma: b.num("sigma_eta")?, epsilon_over_k: b.num("epsilon_over_k")? }),
        (false, false) => None,
        _ => return Err(b.err("sigma_eta and epsilon_over_k come together (CoolProp ignores one alone)")),
    };
    let model = if b.has("type") {
        match b.text("type")? {
            "ECS" => ViscosityModel::Ecs(ecs_viscosity(&mut b)?),
            "rhosr-CS" => ViscosityModel::RhoSrCs(rhosr(&mut b)?),
            "Chung" => ViscosityModel::Chung(chung(&mut b)?),
            other => return unknown(&b, other),
        }
    } else if b.has("hardcoded") {
        let name = b.text("hardcoded")?;
        let model = HardcodedViscosity::from_name(name);
        ViscosityModel::Hardcoded(model.ok_or_else(|| b.err(format!("unknown hardcoded model {name:?}")))?)
    } else {
        ViscosityModel::Staged(staged_viscosity(&mut b)?)
    };
    b.finish()?;
    Ok(Viscosity { source, lennard_jones, model })
}

/// The stages of a staged viscosity: dilute and higher-order terms are required (CoolProp throws at the first call
/// without them), the initial-density term is optional.
fn staged_viscosity(b: &mut Block<'_>) -> Result<StagedViscosity, String> {
    let dilute = viscosity_dilute(b.block("dilute")?)?;
    let initial = if b.has("initial_density") { Some(viscosity_initial(b.block("initial_density")?)?) } else { None };
    let higher = viscosity_higher(b.block("higher_order")?)?;
    Ok(StagedViscosity { dilute, initial, higher })
}

fn viscosity_dilute(mut b: Block<'_>) -> Result<ViscosityDilute, String> {
    let dilute = match kind(&mut b, HardcodedDiluteViscosity::from_name)? {
        Kind::Hardcoded(h) => ViscosityDilute::Hardcoded(h),
        Kind::Type("collision_integral") => ViscosityDilute::CollisionIntegral {
            terms: b.series(["a", "t"])?,
            molar_mass: b.num("molar_mass")?,
            c: b.num("C")?,
        },
        Kind::Type("kinetic_theory") => ViscosityDilute::KineticTheory,
        Kind::Type("powers_of_T") => ViscosityDilute::PowersOfT(b.series(["a", "t"])?),
        Kind::Type("powers_of_Tr") => {
            ViscosityDilute::PowersOfTr { terms: b.series(["a", "t"])?, t_r: b.num("T_reducing")? }
        }
        Kind::Type("collision_integral_powers_of_Tstar") => ViscosityDilute::CollisionIntegralPowersOfTstar {
            terms: b.series(["a", "t"])?,
            t_r: b.num("T_reducing")?,
            c: b.num("C")?,
        },
        Kind::Type(other) => return unknown(&b, other),
    };
    b.finish()?;
    Ok(dilute)
}

fn viscosity_initial(mut b: Block<'_>) -> Result<ViscosityInitial, String> {
    let initial = match b.text("type")? {
        "Rainwater-Friend" => ViscosityInitial::RainwaterFriend(b.series(["b", "t"])?),
        "empirical" => {
            let [n, d, t] = b.parallel(["n", "d", "t"])?.map(Into::into);
            ViscosityInitial::Empirical { n, d, t, t_r: b.num("T_reducing")?, rho_r: b.num("rhomolar_reducing")? }
        }
        other => return unknown(&b, other),
    };
    b.finish()?;
    Ok(initial)
}

fn viscosity_higher(mut b: Block<'_>) -> Result<ViscosityHigher, String> {
    let higher = match kind(&mut b, HardcodedHigherViscosity::from_name)? {
        Kind::Hardcoded(h) => ViscosityHigher::Hardcoded(h),
        Kind::Type("modified_Batschinski_Hildebrand") => ViscosityHigher::BatschinskiHildebrand(Box::new(bh(&mut b)?)),
        Kind::Type("friction_theory") => ViscosityHigher::FrictionTheory(Box::new(friction(&mut b)?)),
        Kind::Type(other) => return unknown(&b, other),
    };
    b.finish()?;
    Ok(higher)
}

fn bh(b: &mut Block<'_>) -> Result<BatschinskiHildebrand, String> {
    let (t_r, rho_r) = (b.num("T_reduce")?, b.num("rhomolar_reduce")?);
    let [a, t1, d1, gamma, l] = b.parallel(["a", "t1", "d1", "gamma", "l"])?.map(Into::into);
    let [f, t2, d2] = b.parallel(["f", "t2", "d2"])?.map(Into::into);
    let [g, h] = b.parallel(["g", "h"])?.map(Into::into);
    let [p, q] = b.parallel(["p", "q"])?.map(Into::into);
    Ok(BatschinskiHildebrand { t_r, rho_r, a, t1, d1, gamma, l, f, t2, d2, g, h, p, q })
}

/// One friction coefficient: its three `A` and its exponent `N`.
fn kappa(b: &mut Block<'_>, a: &str, n: &str) -> Result<Kappa, String> {
    Ok(Kappa { a: b.fixed(a)?, n: b.exponent(n)? })
}

/// Friction theory. CoolProp takes exactly one of `Arr` and `Adrdr`, reads `Aii` with `Nii`, and reads `Aaaa` and
/// `Arrr` only together, silently dropping one alone (`FluidLibrary.h:640-668`). Datagen refuses that: `Aii` and
/// `Arrr` decide, and a partner key without them is an unknown field.
fn friction(b: &mut Block<'_>) -> Result<FrictionTheory, String> {
    let second = match (b.has("Arr"), b.has("Adrdr")) {
        (true, false) => FrictionSecond::Rr(kappa(b, "Arr", "Nrr")?),
        (false, true) => FrictionSecond::Drdr(kappa(b, "Adrdr", "Nrr")?),
        _ => return Err(b.err("friction theory takes exactly one of Arr and Adrdr")),
    };
    let kii = if b.has("Aii") { Some(kappa(b, "Aii", "Nii")?) } else { None };
    let cubic = if b.has("Arrr") {
        Some(FrictionCubic { krrr: kappa(b, "Arrr", "Nrrr")?, kaaa: kappa(b, "Aaaa", "Naaa")? })
    } else {
        None
    };
    let (t_r, c1, c2, ki) = (b.num("T_reduce")?, b.num("c1")?, b.num("c2")?, b.fixed("Ai")?);
    let (ka, kr, kaa) = (kappa(b, "Aa", "Na")?, kappa(b, "Ar", "Nr")?, kappa(b, "Aaa", "Naa")?);
    Ok(FrictionTheory { t_r, c1, c2, ki, ka, kr, kaa, second, kii, cubic })
}

/// An ECS block's ψ: its series in ρ/ρ_r and ρ_r.
fn psi(b: &mut Block<'_>) -> Result<(Series, f64), String> {
    let mut psi = b.block("psi")?;
    let (terms, rho_r) = (psi.series(["a", "t"])?, psi.num("rhomolar_reducing")?);
    psi.finish()?;
    Ok((terms, rho_r))
}

fn ecs_viscosity(b: &mut Block<'_>) -> Result<EcsViscosity, String> {
    let reference = b.text("reference_fluid")?.into();
    let (psi, psi_rho_r) = psi(b)?;
    Ok(EcsViscosity { reference, psi, psi_rho_r })
}

fn rhosr(b: &mut Block<'_>) -> Result<RhoSrCs, String> {
    Ok(RhoSrCs {
        c: b.num("C")?,
        c_liq: b.fixed("c_liq")?,
        c_vap: b.fixed("c_vap")?,
        rhosr_critical: b.num("rhosr_critical")?,
        x_crossover: b.num("x_crossover")?,
    })
}

fn chung(b: &mut Block<'_>) -> Result<Chung, String> {
    Ok(Chung {
        rho_c: b.num("rhomolar_critical")?,
        t_c: b.num("T_critical")?,
        molar_mass: b.num("molar_mass")?,
        dipole_moment: b.num("dipole_moment_D")?,
        acentric: b.num("acentric")?,
        kappa: b.num("kappa")?,
    })
}

fn conductivity(mut b: Block<'_>) -> Result<Conductivity, String> {
    let source = source(&mut b)?;
    let model = if b.has("type") {
        match b.text("type")? {
            "ECS" => ConductivityModel::Ecs(ecs_conductivity(&mut b)?),
            other => return unknown(&b, other),
        }
    } else if b.has("hardcoded") {
        let name = b.text("hardcoded")?;
        let model = HardcodedConductivity::from_name(name);
        ConductivityModel::Hardcoded(model.ok_or_else(|| b.err(format!("unknown hardcoded model {name:?}")))?)
    } else {
        // All three stages: CoolProp throws at the first call without one.
        ConductivityModel::Staged(StagedConductivity {
            dilute: conductivity_dilute(b.block("dilute")?)?,
            residual: conductivity_residual(b.block("residual")?)?,
            critical: conductivity_critical(b.block("critical")?)?,
        })
    };
    b.finish()?;
    Ok(Conductivity { source, model })
}

fn ecs_conductivity(b: &mut Block<'_>) -> Result<EcsConductivity, String> {
    let reference = b.text("reference_fluid")?.into();
    let (psi, psi_rho_r) = psi(b)?;
    let mut f_int = b.block("f_int")?;
    let (terms, f_int_t_r) = (f_int.series(["a", "t"])?, f_int.num("T_reducing")?);
    f_int.finish()?;
    Ok(EcsConductivity { reference, psi, psi_rho_r, f_int: terms, f_int_t_r, q_d: b.num("q_D")? })
}

fn conductivity_dilute(mut b: Block<'_>) -> Result<ConductivityDilute, String> {
    let dilute = match kind(&mut b, HardcodedDiluteConductivity::from_name)? {
        Kind::Hardcoded(h) => ConductivityDilute::Hardcoded(h),
        Kind::Type("ratio_of_polynomials") => ConductivityDilute::RatioOfPolynomials {
            numerator: b.series(["A", "n"])?,
            denominator: b.series(["B", "m"])?,
            t_r: b.num("T_reducing")?,
        },
        Kind::Type("eta0_and_poly") => ConductivityDilute::Eta0AndPoly(b.series(["A", "t"])?),
        Kind::Type(other) => return unknown(&b, other),
    };
    b.finish()?;
    Ok(dilute)
}

/// The residual term. CoolProp's one hardcoded residual term (`RESIDUAL_CO2`) parses and then throws (ROT-116), so
/// this slot has none.
fn conductivity_residual(mut b: Block<'_>) -> Result<ConductivityResidual, String> {
    let residual = match kind(&mut b, |_| None::<Infallible>)? {
        Kind::Hardcoded(never) => match never {},
        Kind::Type("polynomial") => {
            let [coefficients, t, d] = b.parallel(["B", "t", "d"])?.map(Into::into);
            let (t_r, rho_r) = (b.num("T_reducing")?, b.num("rhomass_reducing")?);
            ConductivityResidual::Polynomial { b: coefficients, t, d, t_r, rho_r }
        }
        Kind::Type("polynomial_and_exponential") => {
            let [a, t, d, gamma, l] = b.parallel(["A", "t", "d", "gamma", "l"])?.map(Into::into);
            ConductivityResidual::PolynomialAndExponential { a, t, d, gamma, l }
        }
        Kind::Type(other) => return unknown(&b, other),
    };
    b.finish()?;
    Ok(residual)
}

/// The critical term; the Olchowy-Sengers constants a block omits take CoolProp's defaults (`FluidLibrary.h:918-935`).
fn conductivity_critical(mut b: Block<'_>) -> Result<ConductivityCritical, String> {
    let critical = match kind(&mut b, HardcodedCriticalConductivity::from_name)? {
        Kind::Hardcoded(h) => ConductivityCritical::Hardcoded(h),
        Kind::Type("simplified_Olchowy_Sengers") => {
            let d = OlchowySengers::COOLPROP;
            ConductivityCritical::OlchowySengers(OlchowySengers {
                q_d: b.num_or("qD", d.q_d)?,
                zeta0: b.num_or("zeta0", d.zeta0)?,
                big_gamma: b.num_or("GAMMA", d.big_gamma)?,
                gamma: b.num_or("gamma", d.gamma)?,
                r0: b.num_or("R0", d.r0)?,
                nu: b.num_or("nu", d.nu)?,
                t_ref: if b.has("T_ref") { Some(b.num("T_ref")?) } else { None },
            })
        }
        Kind::Type(other) => return unknown(&b, other),
    };
    b.finish()?;
    Ok(critical)
}
