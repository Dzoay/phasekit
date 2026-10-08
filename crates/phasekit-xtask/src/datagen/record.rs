//! The mirror → core's record (ARCHITECTURE.md §8 steps 2-3; PLAN.md M2.3). Every JSON quirk of map 02 §9 is resolved
//! into closed types here, once:
//!
//! - "0 means absent": a Power term's `l = 0` has no exponential (`c = 0`), `l > 0` has `c = 1`; an Exponential term
//!   with `l = 0` must have `g = 0` (CoolProp would drop a nonzero `g` silently); Lemmon2005 keeps its zero `l`, `m`.
//! - GaoB `η` takes the paper's sign (CoolProp stores −η).
//! - Planck-Einstein θ is positive, as printed: `PlanckEinsteinFunctionT` becomes θ = v/T_crit, computed as CoolProp
//!   computes it; CP0Constant and an Aly-Lee block's constant become c_p⁰ power terms, the Aly-Lee hyperbolic terms
//!   generalized Planck-Einstein terms, exactly as CoolProp converts them (`FluidLibrary.h:285-322`).
//! - Each c_p⁰ block keeps its own `Tc` (R123: 456.82 K against T_r = 456.831 K; map 02 §6).
//! - `t_min` (CoolProp's T_min, the saturation minimum) and the triple point (`EOS.Ttriple`, never read by CoolProp)
//!   stay apart (map 09 R8).
//! - Metadata sentinels become `None` (ROT-057); the environmental block is restricted data.
//!
//! and validated: integer exponents within `MAX_POW`, equal lengths, the units every `*_units` field names, finite
//! values. Only `EOS[0]` is mapped; alternates are listed by [`skipped`] (ROT-041), and an associating block in a
//! default EOS is refused (ROT-068). Derived states (`hmolar`, `smolar` of `STATES`) are never mapped (ROT-052).

use phasekit_core::internal::{
    DoubleExponentialTerm, Environmental, EosRecord, FluidRecord, GaoBTerm, GaussianTerm, IdealTerm, Lemmon2005Term,
    MAX_POW, MeltingSegment, NonAnalyticTerm, OffsetReference, PowerTerm, SaStamp,
};
use phasekit_core::{CriticalOrigin, CriticalPoint, DataTerms, Limits, Source};

use super::mirror::{self, IdealBlock, Num, ResidualBlock, StatePoint};

/// Maps one parsed fluid file to its record.
pub fn to_record(source: &super::Source) -> Result<FluidRecord, String> {
    let file = &source.file;
    let fluid = &source.fluid;
    let at = |what: &str| format!("{file}: {what}");
    let eos = fluid.eos.first().ok_or_else(|| at("no EOS"))?;
    check_units(eos, &fluid.states).map_err(|e| at(&e))?;

    let reducing = &eos.states.reducing;
    let mut record =
        EosRecord::new(eos.gas_constant, reducing.t, reducing.rhomolar, eos.states.sat_min_liquid.rhomolar);
    for (i, block) in eos.alphar.iter().enumerate() {
        residual(block, &mut record).map_err(|e| at(&format!("EOS[0].alphar[{i}]: {e}")))?;
    }
    for (i, block) in eos.alpha0.iter().enumerate() {
        ideal(block, &mut record.ideal).map_err(|e| at(&format!("EOS[0].alpha0[{i}]: {e}")))?;
    }

    let limits = Limits::new(eos.states.sat_min_liquid.t, eos.t_max, eos.p_max).map_err(|e| at(&e.to_string()))?;
    let limits = limits.with_t_triple(eos.t_triple);
    let source = Source::new(eos.bibtex_eos.as_str(), None, DataTerms::Published);
    let info = &fluid.info;
    let mut fluid_record = FluidRecord::new(&info.name, eos.molar_mass, source, record, limits);
    fluid_record.aliases.clone_from(&info.aliases);
    fluid_record.cas = Some(info.cas.clone());
    fluid_record.refprop_name = Some(info.refprop_name.clone()).filter(|n| n != "N/A");
    fluid_record.inchi_key.clone_from(&info.inchi_key);
    // The shipped superancillary was fitted to exactly this EOS: M2.2's FNV-1a gate recomputed its stamp from the
    // parsed JSON. Its stamp here is phasekit's own hash gate (E14; PLAN.md M2.8).
    if eos.superancillary.is_some() {
        let e = &fluid_record.eos;
        let (shape, gas_constant, rho_reducing) = (e.shape_hash(), e.gas_constant, e.rho_reducing);
        fluid_record.superancillary_fit = Some(SaStamp { shape, gas_constant, rho_reducing });
    }
    if let Some(sa) = &eos.superancillary {
        fluid_record.superancillary =
            Some(super::superanc::superancillary(sa).map_err(|e| at(&format!("EOS[0].{e}")))?);
    }
    let c = &fluid.states.critical;
    fluid_record.critical = Some(CriticalPoint { t: c.t, p: c.p, rho: c.rhomolar, origin: CriticalOrigin::Published });
    fluid_record.environmental = info.environmental.as_ref().map(environmental).transpose().map_err(|e| at(&e))?;
    fluid_record.melting = melting(&fluid.ancillaries).map_err(|e| at(&format!("ANCILLARIES.melting_line: {e}")))?;
    check_constants(&fluid_record).map_err(|e| at(&e))?;
    if let Some(sa) = fluid_record.superancillary.clone() {
        fluid_record.caloric = Some(super::caloric::curves(&fluid_record, &sa).map_err(|e| at(&e))?);
    }
    Ok(fluid_record)
}

/// The alternate EOS entries a file carries and datagen does not map, one line each (ROT-041, map 09 R10).
pub fn skipped(source: &super::Source) -> Vec<String> {
    let alternates = source.fluid.eos.iter().enumerate().skip(1);
    alternates
        .map(|(i, eos)| format!("{}: EOS[{i}] ({}) is an alternate, not mapped", source.file, eos.bibtex_eos))
        .collect()
}

/// An integer exponent written as an integer or an integral float, within `0..=MAX_POW` (map 02 §3.1).
fn exponent(x: Num) -> Result<u8, String> {
    let v = x.value();
    if v.fract() != 0.0 || !(0.0..=MAX_POW as f64).contains(&v) {
        return Err(format!("exponent {v} is not an integer in 0..={MAX_POW}"));
    }
    Ok(v as u8)
}

fn exponents(xs: &[Num]) -> Result<Vec<u8>, String> {
    xs.iter().copied().map(exponent).collect()
}

/// Every list of a block has the length of its first.
fn same_lengths(lists: &[(&str, usize)]) -> Result<usize, String> {
    let n = lists.first().map_or(0, |(_, n)| *n);
    match lists.iter().find(|(_, len)| *len != n) {
        Some((name, len)) => Err(format!("{name} has {len} entries, {} has {n}", lists[0].0)),
        None => Ok(n),
    }
}

fn residual(block: &ResidualBlock, eos: &mut EosRecord) -> Result<(), String> {
    match block {
        ResidualBlock::Power { n, t, d, l } => {
            let k = same_lengths(&[("n", n.len()), ("t", t.len()), ("d", d.len()), ("l", l.len())])?;
            let (d, l) = (exponents(d)?, exponents(l)?);
            for i in 0..k {
                let c = if l[i] > 0 { 1.0 } else { 0.0 };
                eos.power.push(PowerTerm::new(n[i], t[i], d[i], l[i], c));
            }
        }
        ResidualBlock::Exponential { n, t, d, l, g } => {
            let k = same_lengths(&[("n", n.len()), ("t", t.len()), ("d", d.len()), ("l", l.len()), ("g", g.len())])?;
            let (d, l) = (exponents(d)?, exponents(l)?);
            for i in 0..k {
                if l[i] == 0 && g[i] != 0.0 {
                    return Err(format!("term {i}: l = 0 with g = {} (CoolProp drops e^(-g))", g[i]));
                }
                eos.power.push(PowerTerm::new(n[i], t[i], d[i], l[i], g[i]));
            }
        }
        ResidualBlock::Lemmon2005 { n, t, d, l, m } => {
            let k = same_lengths(&[("n", n.len()), ("t", t.len()), ("d", d.len()), ("l", l.len()), ("m", m.len())])?;
            let (d, l) = (exponents(d)?, exponents(l)?);
            eos.lemmon2005.extend((0..k).map(|i| Lemmon2005Term { n: n[i], t: t[i], d: d[i], l: l[i], m: m[i] }));
        }
        ResidualBlock::DoubleExponential { n, t, d, gd, ld, gt, lt } => {
            let lists = [("n", n.len()), ("t", t.len()), ("d", d.len()), ("gd", gd.len())];
            let k = same_lengths(&[&lists[..], &[("ld", ld.len()), ("gt", gt.len()), ("lt", lt.len())]].concat())?;
            let (d, ld) = (exponents(d)?, exponents(ld)?);
            eos.double_exponential.extend((0..k).map(|i| DoubleExponentialTerm {
                n: n[i],
                t: t[i],
                d: d[i],
                gd: gd[i],
                ld: ld[i],
                gt: gt[i],
                lt: lt[i],
            }));
        }
        ResidualBlock::Gaussian { n, t, d, eta, epsilon, beta, gamma } => {
            let lists = [("n", n.len()), ("t", t.len()), ("d", d.len()), ("eta", eta.len())];
            let more = [("epsilon", epsilon.len()), ("beta", beta.len()), ("gamma", gamma.len())];
            let k = same_lengths(&[&lists[..], &more[..]].concat())?;
            let d = exponents(d)?;
            eos.gaussian.extend((0..k).map(|i| GaussianTerm {
                n: n[i],
                t: t[i],
                d: d[i],
                eta: eta[i],
                epsilon: epsilon[i],
                beta: beta[i],
                gamma: gamma[i],
            }));
        }
        ResidualBlock::GaoB { n, t, d, eta, epsilon, beta, gamma, b } => {
            let lists = [("n", n.len()), ("t", t.len()), ("d", d.len()), ("eta", eta.len())];
            let more = [("epsilon", epsilon.len()), ("beta", beta.len()), ("gamma", gamma.len()), ("b", b.len())];
            let k = same_lengths(&[&lists[..], &more[..]].concat())?;
            let d = exponents(d)?;
            eos.gao_b.extend((0..k).map(|i| GaoBTerm {
                n: n[i],
                t: t[i],
                d: d[i],
                eta: -eta[i],
                epsilon: epsilon[i],
                beta: beta[i],
                gamma: gamma[i],
                b: b[i],
            }));
        }
        ResidualBlock::NonAnalytic { n, a, b, beta, big_a, big_b, big_c, big_d } => {
            let lists = [("n", n.len()), ("a", a.len()), ("b", b.len()), ("beta", beta.len())];
            let more = [("A", big_a.len()), ("B", big_b.len()), ("C", big_c.len()), ("D", big_d.len())];
            let k = same_lengths(&[&lists[..], &more[..]].concat())?;
            eos.non_analytic.extend((0..k).map(|i| NonAnalyticTerm {
                n: n[i],
                a: a[i],
                b: b[i],
                beta: beta[i],
                big_a: big_a[i],
                big_b: big_b[i],
                big_c: big_c[i],
                big_d: big_d[i],
            }));
        }
        ResidualBlock::Associating { .. } => {
            return Err("ResidualHelmholtzAssociating is not ported (ROT-068; only an alternate EOS uses it)".into());
        }
    }
    Ok(())
}

fn positive(name: &str, x: f64) -> Result<(), String> {
    if x > 0.0 { Ok(()) } else { Err(format!("{name} = {x} is not positive")) }
}

/// CoolProp skips an Aly-Lee coefficient at or below this magnitude (`FluidLibrary.h:288-310`).
const ALY_LEE_ZERO: f64 = 1e-14;

fn ideal(block: &IdealBlock, terms: &mut Vec<IdealTerm>) -> Result<(), String> {
    match block {
        IdealBlock::Lead { a1, a2, .. } => terms.push(IdealTerm::Lead { a1: *a1, a2: *a2 }),
        IdealBlock::LogTau { a } => terms.push(IdealTerm::LogTau { a: *a }),
        IdealBlock::Power { n, t } => {
            let k = same_lengths(&[("n", n.len()), ("t", t.len())])?;
            terms.extend((0..k).map(|i| IdealTerm::Power { n: n[i], t: t[i] }));
        }
        IdealBlock::PlanckEinstein { n, t } => {
            let k = same_lengths(&[("n", n.len()), ("t", t.len())])?;
            terms.extend((0..k).map(|i| IdealTerm::PlanckEinstein { n: n[i], theta: t[i] }));
        }
        IdealBlock::PlanckEinsteinFunctionT { n, v, t_crit, .. } => {
            let k = same_lengths(&[("n", n.len()), ("v", v.len())])?;
            positive("Tcrit", *t_crit)?;
            terms.extend((0..k).map(|i| IdealTerm::PlanckEinstein { n: n[i], theta: v[i] / t_crit }));
        }
        IdealBlock::PlanckEinsteinGeneralized { n, t, c, d } => {
            let k = same_lengths(&[("n", n.len()), ("t", t.len()), ("c", c.len()), ("d", d.len())])?;
            terms.extend((0..k).map(|i| IdealTerm::PlanckEinsteinGeneralized {
                n: n[i],
                theta: t[i],
                c: c[i],
                d: d[i],
            }));
        }
        IdealBlock::EnthalpyEntropyOffset { a1, a2, reference } => {
            let reference = match reference.as_str() {
                "IIR" => OffsetReference::Iir,
                "NBP" => OffsetReference::Nbp,
                "OTH" => OffsetReference::Other,
                "CUSTOM" => OffsetReference::Custom,
                other => return Err(format!("unknown offset reference {other:?}")),
            };
            terms.push(IdealTerm::Offset { a1: *a1, a2: *a2, reference });
        }
        IdealBlock::Cp0PolyT { c, t, tc, t0, .. } => {
            let k = same_lengths(&[("c", c.len()), ("t", t.len())])?;
            terms.extend((0..k).map(|i| IdealTerm::Cp0Power { c: c[i], t: t[i], tc: *tc, t0: *t0 }));
        }
        IdealBlock::Cp0Constant { cp_over_r, tc, t0 } => {
            terms.push(IdealTerm::Cp0Power { c: *cp_over_r, t: 0.0, tc: *tc, t0: *t0 });
        }
        IdealBlock::Cp0AlyLee { c, tc, t0 } => {
            let &[a, b, cc, d, e] = c.as_slice() else {
                return Err(format!("CP0AlyLee has {} constants, not 5", c.len()));
            };
            positive("Tc", *tc)?;
            if a.abs() > ALY_LEE_ZERO {
                terms.push(IdealTerm::Cp0Power { c: a, t: 0.0, tc: *tc, t0: *t0 });
            }
            if b.abs() > ALY_LEE_ZERO {
                terms.push(IdealTerm::PlanckEinsteinGeneralized { n: b, theta: -2.0 * cc / tc, c: 1.0, d: -1.0 });
            }
            if d.abs() > ALY_LEE_ZERO {
                terms.push(IdealTerm::PlanckEinsteinGeneralized { n: -d, theta: -2.0 * e / tc, c: 1.0, d: 1.0 });
            }
        }
    }
    Ok(())
}

/// The restricted environmental block with its sentinels as `None`: a negative GWP or ODP (−1, −9 999 999 999,
/// −99 999 999 999 in v8.0.0), an NFPA rating outside 0-4 (−1, ±999 999 999, 99 999 999, 1e30), and the ASHRAE 34
/// class "UNKNOWN" or "?".
fn environmental(env: &mirror::Environmental) -> Result<Environmental, String> {
    let source = Source::new("CoolProp:INFO.ENVIRONMENTAL", None, DataTerms::Restricted);
    let potential = |x: f64| (x >= 0.0).then_some(x);
    let rating = |x: Num| {
        let v = x.value();
        (v.fract() == 0.0 && (0.0..=4.0).contains(&v)).then_some(v as u8)
    };
    let mut out = Environmental::new(source);
    out.ashrae34 = match env.ashrae34.as_str() {
        "UNKNOWN" | "?" => None,
        class if matches!(class.as_bytes(), [b'A' | b'B', b'1'..=b'3'] | [b'A' | b'B', b'2', b'L']) => {
            Some(class.into())
        }
        other => return Err(format!("ENVIRONMENTAL.ASHRAE34: unknown class {other:?}")),
    };
    (out.gwp20, out.gwp100, out.gwp500, out.odp) =
        (potential(env.gwp20), potential(env.gwp100), potential(env.gwp500), potential(env.odp));
    (out.health, out.flammability, out.physical) = (rating(env.hh), rating(env.fh), rating(env.ph));
    Ok(out)
}

/// The melting curve's segments as stored (30 fluids; map 02 §3.7): each part's reference point and range, in file
/// order. Only these four values are mapped until M8.11 adds the curve forms; a correction of a segment's `p_0`
/// (DIV-0002) needs them now. The ice Ih part of Water keeps CoolProp's T_min > T_max.
fn melting(ancillaries: &serde_json::Value) -> Result<Vec<MeltingSegment>, String> {
    let Some(line) = ancillaries.get("melting_line") else { return Ok(Vec::new()) };
    let parts = line.get("parts").and_then(|p| p.as_array()).ok_or("no parts")?;
    let mut segments = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        let get = |key: &str| part.get(key).and_then(serde_json::Value::as_f64).ok_or(format!("part {i}: no {key}"));
        segments.push(MeltingSegment { t0: get("T_0")?, p0: get("p_0")?, t_min: get("T_min")?, t_max: get("T_max")? });
    }
    Ok(segments)
}

/// The units every `*_units` field must name; datagen checks them and drops them (map 09 §9 D2).
fn check_units(eos: &mirror::Eos, states: &mirror::States) -> Result<(), String> {
    let mut wrong = Vec::new();
    let mut want = |field: &str, got: &str, unit: &str| {
        if got != unit {
            wrong.push(format!("{field} is {got:?}, not {unit:?}"));
        }
    };
    want("gas_constant_units", &eos.gas_constant_units, "J/mol/K");
    want("molar_mass_units", &eos.molar_mass_units, "kg/mol");
    want("acentric_units", &eos.acentric_units, "-");
    want("T_max_units", &eos.t_max_units, "K");
    want("p_max_units", &eos.p_max_units, "Pa");
    want("Ttriple_units", &eos.t_triple_units, "K");
    let s = &eos.states;
    let points = [&s.reducing, &s.sat_min_liquid, &s.sat_min_vapor, &states.critical, &states.triple_liquid]
        .into_iter()
        .chain([&states.triple_vapor])
        .chain(s.hs_anchor.iter().chain(&s.pressure_max_sat).chain(&s.temperature_max_sat));
    for point in points {
        state_units(point, &mut want);
    }
    for block in &eos.alpha0 {
        if let IdealBlock::PlanckEinsteinFunctionT { t_crit_units: Some(unit), .. } = block {
            want("Tcrit_units", unit, "K");
        }
    }
    if wrong.is_empty() { Ok(()) } else { Err(wrong.join("; ")) }
}

fn state_units(point: &StatePoint, want: &mut impl FnMut(&str, &str, &str)) {
    want("T_units", &point.t_units, "K");
    want("p_units", &point.p_units, "Pa");
    want("rhomolar_units", &point.rhomolar_units, "mol/m^3");
    if let Some(unit) = &point.hmolar_units {
        want("hmolar_units", unit, "J/mol");
    }
    if let Some(unit) = &point.smolar_units {
        want("smolar_units", unit, "J/mol/K");
    }
}

/// The constants are finite and positive. JSON cannot write a non-finite number, and the only divisions of the
/// conversions are by a positive `Tcrit` or `Tc` (checked there), so every term value is finite too.
fn check_constants(record: &FluidRecord) -> Result<(), String> {
    let e = &record.eos;
    let constants = [
        ("molar_mass", record.molar_mass),
        ("gas_constant", e.gas_constant),
        ("T_reducing", e.t_reducing),
        ("rhomolar_reducing", e.rho_reducing),
        ("rho_max", e.rho_max),
    ];
    match constants.iter().find(|(_, x)| !(x.is_finite() && *x > 0.0)) {
        Some((name, x)) => Err(format!("{name} = {x} is not finite and positive")),
        None => Ok(()),
    }
}
