//! The multi-fluid mixtures of `phasekit-mix` (PLAN.md M13) against Bell et al.'s printed check tables (M13.3).
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Basis, Bundle, DataSet, DomainPolicy, FlashOptions, Fluid, Input, Order, Pair, Phase, Registry};
use phasekit_mix::{Mixture, MultiFluid};
use phasekit_verify::eos::Majorants;
use phasekit_verify::term::IdealScale;
use phasekit_verify::{CRITICAL_NOT_REDUCING, DIVERGENCES, Fixture, fixture, majorant};

/// The library's name of a fluid the tables spell as REFPROP does; `NEWR1234YF` is the R1234yf equation both tables use
/// (Table XIII's pure row is v8.0.0's to all printed digits; Table XI's predates it, DIV-0050).
fn library_name(printed: &str) -> &str {
    match printed {
        "NEWR1234YF" => "R1234yf",
        "R1234ZEE" => "R1234ze(E)",
        other => other,
    }
}

/// A row's (T_r, ρ_r, α^r) at its (T, ρ): a pure fluid's from its own model, a pair's from the multi-fluid model at
/// (z₁, 1 − z₁).
fn evaluate(registry: &Registry, data: DataSet, table: &Fixture<'_>, row: usize) -> (f64, f64, f64) {
    let fluids = table.printed(row, "fluids").unwrap();
    let (t, rho, z1) =
        (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap(), table.value(row, "z1").unwrap());
    let names: Vec<&str> = fluids.split('&').map(library_name).collect();
    if let [one] = names[..] {
        let eos = registry.get(one).unwrap().model().helmholtz().unwrap();
        let reducing = eos.reducing().unwrap();
        return (reducing.t, reducing.rho, eos.residual(t, rho, Order::One).get(0, 0).unwrap());
    }
    let mix = MultiFluid::new(registry, &names, data).unwrap();
    let x = [z1, 1.0 - z1];
    let (t_r, rho_r) = mix.reducing_state(&x);
    (t_r, rho_r, mix.residual(&x, t, rho, Order::One).unwrap().get(0, 0).unwrap())
}

/// Four ulp, relative.
const ULP4: f64 = 4.0 * f64::EPSILON;

/// `got` against a cell: within its printed digits or `rounding` of |got|, whichever is larger.
fn within(table: &Fixture<'_>, row: usize, column: &str, got: f64, rounding: f64) -> Result<(), String> {
    let printed = phasekit_verify::from_printed(table.printed(row, column).unwrap());
    let Some(phasekit_verify::Tolerance::Absolute(half)) = printed else { return Err(format!("{printed:?}")) };
    table.check_bound(row, column, got, half.max(rounding * got.abs())).map(|_| ()).map_err(|e| format!("{e:?}"))
}

/// Every row's T_r, ρ_r and α^r, or the cells that miss. The tables print what teqp or CoolProp computed, to 13 and 14
/// decimals, which is below what two double-precision evaluations agree to: each cell holds within its printed digits
/// (class `Paper`) or, if that is tighter, within 4 ulp for T_r and ρ_r (a few roundings of the reducing sums) and
/// within `Term`'s 1e-13 for α^r taken at |α^r|, which never exceeds Σ_k |φ_k|, so the bound is stricter than `Term`.
fn misses(table: &Fixture<'_>, rows: impl Iterator<Item = usize>, data: DataSet) -> Vec<String> {
    // See `within`.
    let registry = Registry::from_embedded(data).unwrap();
    let mut misses = Vec::new();
    for row in rows {
        let (t_r, rho_r, alphar) = evaluate(&registry, data, table, row);
        for (column, got, rounding) in [("Tred", t_r, ULP4), ("rhored", rho_r, ULP4), ("alphar", alphar, 1e-13)] {
            if let Err(e) = within(table, row, column, got, rounding) {
                misses.push(format!("{} {column}: {e}", table.printed(row, "fluids").unwrap()));
            }
        }
    }
    misses
}

/// Bell et al. 2023 Table XIII (teqp 0.14.1): its six pure fluids and five pairs at z₁ = 0.4, (τ, δ) ≈ (0.8, 0.8): the
/// reducing state and α^r of each within its printed digits.
#[test]
fn alphar_matches_bell_2023_table_xiii() {
    let (path, text) = fixture!("paper/mixtures/Bell-JPCRD-2023.XIII.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!(table.rows().len(), 11);
    let misses = misses(&table, 0..11, DataSet::Corrected);
    assert!(misses.is_empty(), "{misses:#?}");
}

/// Bell et al. 2022 Table XI (CoolProp 6.4.2dev) arbitrates the paper's three final models, R1234yf/R134a,
/// R1234yf/R1234ze(E) (DIV-0013's) and R134a/R1234ze(E), which v8.0.0 carries; its interim R125/R1234yf,
/// R1234ze(E)/R227ea and R1234yf/R152a are superseded by Bell et al. 2023, whose Table XIII arbitrates them (R1234ze(E)/
/// R227ea's T_r moved by 0.78 K). The rows without R1234yf hold within their digits; the R1234yf rows used a
/// pre-publication R1234yf equation, so their α^r is DIV-0050's proof and only their reducing states are checked here.
#[test]
fn alphar_matches_bell_2022_table_xi() {
    let (path, text) = fixture!("paper/mixtures/Bell-JPCRD-2022.XI.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!(table.rows().len(), 12);
    let interim = ["R125&NEWR1234YF", "NEWR1234YF&R152A", "R1234ZEE&R227EA"];
    let fluids = |row: usize| table.printed(row, "fluids").unwrap();
    let final_models = (0..12).filter(|&row| !interim.contains(&fluids(row)));
    let (with_r1234yf, without): (Vec<usize>, Vec<usize>) =
        final_models.partition(|&row| fluids(row).contains("NEWR1234YF"));
    assert_eq!((with_r1234yf.len(), without.len()), (3, 6));
    let misses = misses(&table, without.into_iter(), DataSet::Corrected);
    assert!(misses.is_empty(), "{misses:#?}");
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    for row in with_r1234yf {
        let (t_r, rho_r, _) = evaluate(&registry, DataSet::Corrected, &table, row);
        within(&table, row, "Tred", t_r, ULP4).unwrap();
        within(&table, row, "rhored", rho_r, ULP4).unwrap();
    }
}

/// The oracle's columns and the A_ij each holds.
const ALPHAR_COLUMNS: [(&str, usize, usize); 10] = [
    ("alphar", 0, 0),
    ("A10", 1, 0),
    ("A01", 0, 1),
    ("A20", 2, 0),
    ("A11", 1, 1),
    ("A02", 0, 2),
    ("A30", 3, 0),
    ("A21", 2, 1),
    ("A12", 1, 2),
    ("A03", 0, 3),
];

/// A `mix_alphar` or `mix_eos` row's mixture and its components' names: a pair's at (z₁, 1 − z₁), or a predefined
/// mixture's by its `.mix` name, at its own composition (PLAN.md M13.13).
fn row_mixture(registry: &Registry, label: &str, z1: f64) -> Result<(Fluid, Vec<String>), phasekit_core::Error> {
    let fluid = match label.strip_suffix(".mix") {
        Some(name) => phasekit_mix::predefined(registry, name, DataSet::Parity)?,
        None => {
            let names: Vec<&str> = label.split('&').collect();
            phasekit_mix::mixture(registry, &[(names[0], z1), (names[1], 1.0 - z1)], DataSet::Parity)?
        }
    };
    let names = mixture_of(&fluid).model().fluids().map(|f| f.info().name().to_string()).collect();
    Ok((fluid, names))
}

/// The mixture behind `fluid`.
fn mixture_of(fluid: &Fluid) -> &Mixture {
    (fluid.model() as &dyn std::any::Any).downcast_ref::<Mixture>().unwrap()
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/mix_alphar.csv (PLAN.md M13.3). Every binary pair of v8.0.0's
/// library at z₁ = 0.4 and two states of its reducing state, on `Parity`: the reducing state and α^r with its A_ij to
/// order 3 within `Prop`: 1e-12 of the larger of |A_ij| and 1, or `Term` (1e-13 of Σ_k |φ_k|, the components' terms'
/// majorants weighted by x_i) if that is larger, as VERIFICATION.md §5 carries it (CO₂/R124's A12 at δ = 1.6 cancels to
/// 1.4e-12 of its value). The 48 pairs the oracle cannot build, which name a fluid outside v8.0.0's library, phasekit
/// refuses too. The predefined mixtures by `.mix` name (M13.13), at their own compositions: the 116 v8.0.0 builds
/// within the same bounds, the 31 it cannot (ROT-200: 27 lack a binary pair, 4 name R1132a) refused here too.
#[test]
fn mix_alphar_matches_the_oracle() {
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_alphar.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut compared, mut refused, mut misses) = (0, 0, Vec::new());
    for row in 0..table.rows().len() {
        let label = table.printed(row, "mixture").unwrap();
        let built = row_mixture(&registry, label, table.value(row, "z1").unwrap());
        if table.printed(row, "status") != Some("ok") {
            assert!(built.is_err(), "{label}: the oracle refuses it, phasekit builds it");
            refused += 1;
            continue;
        }
        let (fluid, names) = built.unwrap();
        let mixture = mixture_of(&fluid);
        let (mix, x) = (mixture.model(), mixture.composition());
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rhomolar").unwrap());
        let (t_r, rho_r) = mix.reducing_state(x);
        let d = mix.residual(x, t, rho, Order::Three).unwrap();
        for (column, got) in [("Tr", t_r), ("rhor", rho_r)] {
            if let Err(e) = table.check_scaled(row, column, got, got.abs()) {
                misses.push(format!("{label} row {row} {column}: {e:?}"));
            }
        }
        let records: Vec<_> = names.iter().map(|n| phasekit_core::internal::record(&registry, n).unwrap()).collect();
        let (tau, delta) = (t_r / t, rho / rho_r);
        for (column, i, j) in ALPHAR_COLUMNS {
            let majorant: f64 = records
                .iter()
                .zip(x)
                .map(|(r, xi)| xi * phasekit_verify::majorant::eos(&r.eos, tau, delta, i, j))
                .sum();
            let scale = table.value(row, column).unwrap().abs().max(1.0).max(0.1 * majorant);
            if let Err(e) = table.check_scaled(row, column, d.get(i, j).unwrap(), scale) {
                misses.push(format!("{label} row {row} {column}: {e:?}"));
            }
        }
        compared += 1;
    }
    assert!(misses.is_empty(), "{} misses: {:#?}", misses.len(), &misses[..misses.len().min(20)]);
    assert_eq!((compared, refused), (1680 + 2 * 116, 96 + 2 * 31));
}

/// A table row's composition, `50/50` or `33/33/34` in mole percent, as fractions.
fn fractions(printed: &str) -> Vec<f64> {
    printed.split('/').map(|x| x.parse::<f64>().unwrap() / 100.0).collect()
}

/// Tkaczuk et al. 2020 Table 8 (CoolProp 6.3.0, which "reproduce[s] exactly" it): p of the three equimolar noble-gas
/// pairs at 200 K and 10 mol/dm³, computed with the mole-fraction-weighted R of the paper's Sec. 2, Σ x_i R_i.
/// phasekit's α^r gives each through that R within its printed digits or 1e-13 of p, whichever is larger; its own p,
/// with CoolProp's R_CODATA (user decision MX3), is DIV-0051's proof.
#[test]
fn pressures_match_tkaczuk_2020_table_8() {
    let (path, text) = fixture!("paper/mixtures/Tkaczuk-JPCRD-2020.8.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    assert_eq!(table.rows().len(), 3);
    for row in 0..3 {
        let names: Vec<&str> = table.printed(row, "fluids").unwrap().split('&').collect();
        let (t, rho, z1) =
            (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() * 1e3, table.value(row, "z1").unwrap());
        let x = [z1, 1.0 - z1];
        let mix = MultiFluid::new(&registry, &names, DataSet::Corrected).unwrap();
        let r: f64 = mix.fluids().zip(&x).map(|(f, xi)| xi * f.model().helmholtz().unwrap().gas_constant()).sum();
        let a01 = mix.residual(&x, t, rho, Order::One).unwrap().get(0, 1).unwrap();
        let p = rho * r * t * (1.0 + a01);
        within(&table, row, "p", p, 1e-13).unwrap();
    }
}

/// Lemmon and Jacobsen 2004 Table 6 (REFPROP, 22 rows: six binaries at 50/50, two ternaries at 33/33/34): p, c_v,
/// c_p, w and the first component's fugacity f₁ = x₁ φ₁ p (M13.6) at each (T, ρ, x). It used the components' R;
/// phasekit's model uses R_CODATA (user decision MX3), so each cell holds within DIV-0054's measured 3e-6 away from the
/// critical point, and the six rows near it within the table's 1e-5 in p, c_v, w and f₁; c_p, which diverges there, is
/// not asserted on them.
#[test]
fn properties_match_lemmon_2004_table_6() {
    let (path, text) = fixture!("paper/mixtures/Lemmon-JPCRD-2004.6.csv");
    let table = Fixture::parse(path, text).unwrap();
    assert_eq!(table.rows().len(), 22);
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0054").unwrap();
    let relative = |t: Option<phasekit_verify::Tolerance>| match t {
        Some(phasekit_verify::Tolerance::Relative(x)) => x,
        _ => f64::NAN,
    };
    let (away, near) = (relative(entry.tolerance), relative(entry.table_tolerances.first().map(|(_, t)| *t)));
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let mut misses = Vec::new();
    for row in 0..22 {
        let names: Vec<&str> = table.printed(row, "mixture").unwrap().split('&').collect();
        let parts: Vec<(&str, f64)> = names.iter().copied().zip(fractions(table.printed(row, "x").unwrap())).collect();
        let fluid = phasekit_mix::mixture(&registry, &parts, DataSet::Corrected).unwrap();
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap() * 1e3);
        let s = fluid
            .flash(Input::new(Pair::DT, rho, t, Basis::Molar).unwrap(), &FlashOptions::new().with_phase(Phase::Gas))
            .unwrap();
        let near_critical = table.printed(row, "note").is_some_and(|n| !n.is_empty());
        let mix = (fluid.model() as &dyn std::any::Any).downcast_ref::<phasekit_mix::Mixture>().unwrap();
        let ln_phi = mix.composition_at(t, rho, Order::One).unwrap().ln_phi(0);
        let cells = [
            ("p", s.p() / 1e6),
            ("cv", s.cv(Basis::Molar).unwrap()),
            ("cp", s.cp(Basis::Molar).unwrap()),
            ("w", s.speed_of_sound().unwrap()),
            ("f1", mix.composition()[0] * phasekit_core::math::exp(ln_phi) * s.p() / 1e6),
        ];
        for (column, got) in cells.into_iter().filter(|(c, _)| !(near_critical && *c == "cp")) {
            let want = table.value(row, column).unwrap();
            let bound = if near_critical { near } else { away };
            if (got / want - 1.0).abs() > bound {
                misses.push(format!("row {row} {column}: {got} against {want}"));
            }
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
}

/// The embedded fluids whose published critical temperature is not their EOS's reducing one: v8.0.0 evaluates their
/// α⁰ in a mixture at τ_i = T_c,i τ/T_r (map 15 X12), phasekit at their own τ (ROT-062).
fn critical_not_reducing(registry: &Registry) -> Vec<String> {
    let mut names = Vec::new();
    for name in registry.names() {
        let fluid = registry.get(name).unwrap();
        let (Some(c), Some(r)) =
            (fluid.info().published_critical(), fluid.model().helmholtz().and_then(|m| m.reducing()))
        else {
            continue;
        };
        if c.t != r.t {
            names.push(name.to_string());
        }
    }
    names.sort();
    names
}

/// The `Term` scale of a mixture's bundle at (T, ρ): its components' majorants at the mixture's τ and δ (the
/// residual) and at (T, ρ) (the ideal part, by R_i/R), weighted by x. The departure functions are left out, so the
/// bound is if anything tighter than `Term`'s.
fn majorants(registry: &Registry, names: &[&str], x: &[f64], (tau, delta): (f64, f64), t: f64, rho: f64) -> Majorants {
    let mut m = [0.0; 6];
    for (name, xi) in names.iter().zip(x) {
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let ideal = IdealScale::new(&record).unwrap();
        let ratio = record.eos.gas_constant / phasekit_mix::R_CODATA;
        for (k, (i, j)) in [(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2)].into_iter().enumerate() {
            m[k] += xi * (majorant::eos(&record.eos, tau, delta, i, j) + ratio * ideal.get(t, rho, i, j));
        }
    }
    let [m00, m10, m01, m20, m11, m02] = m;
    Majorants { m00, m10, m01, m20, m11, m02 }
}

/// A `mix_eos` row's state, evaluated: its columns' values, and the bundle and majorants its carried bound reads.
struct Evaluated {
    cells: [(&'static str, f64); 11],
    total: Bundle,
    majorants: Majorants,
    molar_mass: f64,
}

/// `fluid` (the mixture of `names` at `x`) at (T, ρ) with the gas phase imposed, as the oracle evaluates it, outside
/// its components' limits too (phasekit only when asked, ROT-197).
fn evaluate_state(
    registry: &Registry,
    fluid: &phasekit_core::Fluid,
    names: &[&str],
    x: &[f64],
    t: f64,
    rho: f64,
) -> Evaluated {
    let options = FlashOptions::new().with_phase(Phase::Gas).with_domain(DomainPolicy::Extrapolate);
    let s = fluid.flash(Input::new(Pair::DT, rho, t, Basis::Molar).unwrap(), &options).unwrap();
    let eos = fluid.model().helmholtz().unwrap();
    let ideal = eos.ideal(t, rho, Order::Two);
    let total = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
    let reducing = eos.reducing().unwrap();
    let majorants = majorants(registry, names, x, (reducing.t / t, rho / reducing.rho), t, rho);
    let cells = [
        ("p", s.p()),
        ("hmolar", s.h(Basis::Molar)),
        ("smolar", s.s(Basis::Molar)),
        ("umolar", s.u(Basis::Molar)),
        ("cvmolar", s.cv(Basis::Molar).unwrap_or(f64::NAN)),
        ("cpmolar", s.cp(Basis::Molar).unwrap_or(f64::NAN)),
        ("speed_sound", s.speed_of_sound().unwrap_or(f64::NAN)),
        ("Z", s.z()),
        ("alpha0", ideal.get(0, 0).unwrap()),
        ("A0_10", ideal.get(1, 0).unwrap()),
        ("A0_20", ideal.get(2, 0).unwrap()),
    ];
    Evaluated { cells, total, majorants, molar_mass: fluid.info().molar_mass() }
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/mix_eos.csv (PLAN.md M13.4). Every binary pair of v8.0.0's
/// library at z₁ = 0.4 and two states of its reducing state, on `Parity`: p, h, u, c_v, c_p, w, Z and the ideal part's
/// τ-derivatives within `Prop`, or `Term` carried through each relation on the components' majorants if larger. Not
/// asserted: s and α⁰ everywhere (DIV-0052) and every α⁰-derived column of a mixture with one of DIV-0053's fluids; the
/// pairs the oracle refuses phasekit refuses too. The predefined mixtures by `.mix` name (M13.13), at their own
/// compositions, likewise: the 116 v8.0.0 builds compared, the 31 it cannot refused (ROT-200).
#[test]
fn mix_eos_matches_the_oracle() {
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_eos.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let exempt = |id: &str| DIVERGENCES.iter().find(|d| d.id == id).and_then(|d| d.exempt).unwrap().columns;
    let (all, critical) = (exempt("DIV-0052"), exempt("DIV-0053"));
    let (mut compared, mut skipped, mut refused, mut misses) = (0, 0, 0, Vec::new());
    for row in 0..table.rows().len() {
        let label = table.printed(row, "mixture").unwrap();
        let built = row_mixture(&registry, label, table.value(row, "z1").unwrap());
        if table.printed(row, "status") != Some("ok") {
            assert!(built.is_err(), "{label}: the oracle refuses it, phasekit builds it");
            refused += 1;
            continue;
        }
        let (fluid, names) = built.unwrap();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let with_critical = names.iter().any(|n| CRITICAL_NOT_REDUCING.contains(n));
        let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rhomolar").unwrap());
        let e = evaluate_state(&registry, &fluid, &names, mixture_of(&fluid).composition(), t, rho);
        let (kept, left): (Vec<_>, Vec<_>) =
            e.cells.iter().partition(|(c, _)| !(all.contains(c) || (with_critical && critical.contains(c))));
        skipped += left.len();
        for (column, got) in kept {
            let want = table.value(row, column).unwrap();
            let carried = phasekit_verify::eos::carried(
                column,
                &e.total,
                &e.majorants,
                phasekit_mix::R_CODATA,
                t,
                rho,
                e.molar_mass,
            );
            let scale = want.abs().max(1.0).max(if carried.is_nan() { 0.0 } else { 0.1 * carried });
            if let Err(err) = table.check_scaled(row, column, got, scale) {
                misses.push(format!("{label} row {row} {column}: {err:?}"));
            }
            compared += 1;
        }
    }
    assert!(misses.is_empty(), "{} misses: {:#?}", misses.len(), &misses[..misses.len().min(12)]);
    assert_eq!((compared, skipped, refused), (11566, 9466, 96 + 2 * 31));
}

/// DIV-0053's fluids are exactly the embedded ones whose published critical temperature is not their reducing one.
#[test]
fn critical_not_reducing_lists_every_such_fluid() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    assert_eq!(critical_not_reducing(&registry), CRITICAL_NOT_REDUCING.map(String::from).to_vec());
}

/// ROT-062, ROT-103 (map 15 X12): each of DIV-0053's 24 fluids that a binary pair names (21: Air, Fluorine and SES36
/// have none), all but pure in a mixture (x = 1 − 1e-9 beside a library partner), has its own ideal part: the mixture's τ-derivatives A10 and A20 of α⁰ are the fluid's, weighted by
/// R_i/R, within 1e-8 (the partner's share), where v8.0.0 evaluates them at τ_i = T_c,i τ/T_r.
#[test]
fn trace_mixture_equals_pure() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let names: Vec<&str> = registry.names().collect();
    let pair = |a: &str, b: &str| a != b && MultiFluid::new(&registry, &[a, b], DataSet::Corrected).is_ok();
    let mut alone = Vec::new();
    for name in CRITICAL_NOT_REDUCING {
        let Some(partner) = names.iter().find(|p| pair(name, p)) else {
            alone.push(name);
            continue;
        };
        let mix = phasekit_mix::mixture(&registry, &[(name, 1.0 - 1e-9), (partner, 1e-9)], DataSet::Corrected).unwrap();
        let pure = registry.get(name).unwrap().model().helmholtz().unwrap();
        let ratio = pure.gas_constant() / phasekit_mix::R_CODATA;
        let (t, rho) = (1.3 * pure.reducing().unwrap().t, 0.2 * pure.reducing().unwrap().rho);
        let (a, b) = (mix.model().helmholtz().unwrap().ideal(t, rho, Order::Two), pure.ideal(t, rho, Order::Two));
        for i in [1, 2] {
            let (got, want) = (a.get(i, 0).unwrap(), ratio * b.get(i, 0).unwrap());
            assert!(((got - want) / want).abs() <= 1e-8, "{name} with {partner}: A{i}0 {got} against {want}");
        }
    }
    assert_eq!(alone, ["Air", "Fluorine", "SES36"], "the fluids no binary pair of v8.0.0's library names");
}
