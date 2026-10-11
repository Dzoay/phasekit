//! T+X along the isotherm (PLAN.md M7.5).

use super::*;

/// The T+X input of `var` at T for `state`'s value of it.
#[cfg(feature = "fluids-all")]
fn t_x(var: Caloric, state: &State) -> Input {
    let (t, m) = (Temperature::new(state.t()).unwrap(), Basis::Molar);
    match var {
        Caloric::H => Input::ht(Enthalpy::molar(state.h(m)).unwrap(), t),
        Caloric::S => Input::st(Entropy::molar(state.s(m)).unwrap(), t),
        Caloric::U => Input::tu(t, InternalEnergy::molar(state.u(m)).unwrap()),
    }
}

/// ROT-071 (map 03 §6): along an isotherm x(ρ) need not be monotonic. CoolProp's HT of Water at 300 K for the h of
/// its compressed liquid at 1, 10 and 50 MPa silently returns the two-phase state at psat (3536.8 Pa), Q =
/// 0.000376, 0.00376 and 0.0185. Here HT under `Strict` is `Ambiguous` with every root, ascending in ρ: CoolProp's
/// two-phase one (its ρ within 1e-9, its Q within 1e-12) and the compressed liquid; `Nearest` takes each, and each
/// reproduces h. So for ST and TU at 275 K, where water's density maximum makes s and u rise with ρ just above ρ′
/// and fall again: at 1 and 10 MPa CoolProp returns the two-phase root alone, while the isotherm also holds a
/// third, denser liquid (ST at 20.0 and 10.8 MPa, TU at 45.6 and 35.7 MPa; CoolProp's own s and u there are the
/// input's within 3e-11). At 50 MPa the liquid is the only root, where CoolProp returns a liquid under tension, at
/// −25.7 MPa (ST) and −2.8 MPa (TU).
#[cfg(feature = "fluids-all")]
#[test]
fn compressed_liquid_ht_is_ambiguous() {
    let water = embedded("Water");
    let m = Basis::Molar;
    let psat = |t: f64| if t == 300.0 { 3536.8067523441227 } else { 698.4511667637504 }; // CoolProp's
    let none = f64::NAN;
    // (x, T, p, CoolProp's two-phase ρ and Q, the third root's p)
    let cases = [
        (Caloric::H, 300.0, 1e6, 3533.580893560894, 0.00037631464632617855, none),
        (Caloric::H, 300.0, 10e6, 375.0126743823031, 0.0037621392030636916, none),
        (Caloric::H, 300.0, 50e6, 76.50586554004933, 0.018541264669263688, none),
        (Caloric::S, 275.0, 1e6, 33156.925374980456, 3.711509509545968e-06, 1.999518e7),
        (Caloric::S, 275.0, 10e6, 11979.184661349753, 2.0009242162517233e-05, 1.077094e7),
        (Caloric::U, 275.0, 1e6, 32097.120938083248, 4.015902380672754e-06, 4.563345e7),
        (Caloric::U, 275.0, 10e6, 8271.315714431435, 3.144780841976366e-05, 3.566138e7),
    ];
    let near = |got: f64, want: f64, tol: f64| (got / want - 1.0).abs() < tol;
    let x_of = |var: Caloric, state: &State| match var {
        Caloric::H => state.h(m),
        Caloric::S => state.s(m),
        Caloric::U => state.u(m),
    };
    for (var, t, p, rho_2, q_2, p_3) in cases {
        let liquid = water.state(pt(p, t)).unwrap();
        let input = t_x(var, &liquid);
        let Err(Error::Ambiguous { roots }) = water.state(input) else { panic!("{input:?}: {:?}", water.state(input)) };
        let at = |rho: f64| water.flash(input, &FlashOptions::new().with_roots(RootPolicy::Nearest(rho))).unwrap();
        let states: Vec<State> = roots.as_slice().iter().map(|&rho| at(rho)).collect();
        let [two_phase, compressed, rest @ ..] = states.as_slice() else { panic!("{input:?}: {roots:?}") };
        let q = two_phase.quality();
        assert!(near(two_phase.rho(m), rho_2, 1e-9) && q.is_some_and(|q| (q - q_2).abs() < 1e-12), "{two_phase:?}");
        assert!(near(two_phase.p(), psat(t), 1e-9), "{two_phase:?}");
        assert!(near(compressed.rho(m), liquid.rho(m), 1e-9) && compressed.phase() == liquid.phase(), "{compressed:?}");
        match rest {
            [] => assert!(p_3.is_nan(), "{input:?}: no third root"),
            [third] => assert!(near(third.p(), p_3, 1e-6) && third.rho(m) > liquid.rho(m), "{third:?}"),
            _ => panic!("{input:?}: {roots:?}"),
        }
        for state in &states {
            let (got, want) = (x_of(var, state), x_of(var, &liquid));
            assert!((got - want).abs() <= 1e-9 * want.abs().max(1.0), "{input:?}: {got} against {want}");
        }
    }
    for var in [Caloric::S, Caloric::U] {
        let liquid = water.state(pt(50e6, 275.0)).unwrap();
        let unique = water.state(t_x(var, &liquid)).unwrap();
        assert!(near(unique.rho(m), liquid.rho(m), 1e-9) && unique.phase() == liquid.phase(), "{unique:?}");
    }
}

/// ROT-077 (map 03 §6): CoolProp's density solve above Tc has an absolute tolerance, 1e-9 mol/m³, so its ST of
/// Water at 1.3 Tc is 6.1 % off at ρ = 1e-8 mol/m³ and 0.87 % off at 1e-7. Here the isotherm's axis is ln ρ: ST
/// returns ρ = 1e-8, 1e-7 and 1e-6 within class `Flash` (1e-9); HT within the conditioning of h there, the rounding
/// of h over (∂h/∂ρ)_T: h moves 1e-12 of itself between ρ = 0 and 1e-8 (CoolProp 5.6e-4 off, 4e-4 its
/// conditioning).
#[cfg(feature = "fluids-all")]
#[test]
fn low_density_round_trips() {
    let water = embedded("Water");
    let (t, m) = (1.3 * 647.096, Basis::Molar);
    for rho in [1e-8, 1e-7, 1e-6] {
        let truth = water.state(dt(rho, t)).unwrap();
        let st = water.state(t_x(Caloric::S, &truth)).unwrap();
        assert!((st.rho(m) / rho - 1.0).abs() < 1e-9, "ST at {rho}: {}", st.rho(m));
        let ht = water.state(t_x(Caloric::H, &truth)).unwrap();
        let slope = truth.partial(Partial { of: DerivVar::Hmolar, wrt: DerivVar::Dmolar, at: DerivVar::T }).unwrap();
        let conditioning = 4.0 * f64::EPSILON * truth.h(m).abs() / slope.abs();
        assert!((ht.rho(m) - rho).abs() <= conditioning.max(1e-9 * rho), "HT at {rho}: {} ({conditioning})", ht.rho(m));
    }
}

/// The density of the extremum of x along Water's isotherm at T in [lo, hi] (a minimum if `min`), by a scan in ln ρ
/// with steps of 1e-5; states DT refuses are skipped.
#[cfg(feature = "fluids-all")]
fn turn_of_x(water: &Fluid, var: Caloric, t: f64, (lo, hi): (f64, f64), min: bool) -> f64 {
    let steps = (math::ln(hi / lo) / 1e-5) as u32;
    let x = |rho: f64| water.state(dt(rho, t)).ok().map(|s| var.of(&s));
    let scan = (0..=steps).map(|k| lo * math::exp(f64::from(k) * 1e-5)).filter_map(|rho| Some((rho, x(rho)?)));
    let pick = |a: &(f64, f64), b: &(f64, f64)| if min { a.1.total_cmp(&b.1) } else { b.1.total_cmp(&a.1) };
    scan.min_by(pick).unwrap().0
}

/// M7.5: two roots of x along an isotherm within one grid cell, either side of a turn of x, are both found: the
/// cell is cut where (∂x/∂ln ρ)_T changes sign. Water's h at 1000 K has a minimum in ρ, and its s at 275 K a
/// maximum just above ρ′ (by its density maximum). HT and ST for the x 5e-4 past each turn in ρ return the root
/// there and the one just short of the turn, both within 2e-3 of it.
#[cfg(feature = "fluids-all")]
#[test]
fn t_x_finds_both_roots_by_a_turn() {
    let water = embedded("Water");
    let liquid = record_of("Water").superancillary_curve().unwrap().at_t(275.0).unwrap().bubble.rho;
    let cases = [(Caloric::H, 1000.0, (1e3, 6e4), true), (Caloric::S, 275.0, (liquid, 1.2 * liquid), false)];
    for (var, t, range, min) in cases {
        let turn = turn_of_x(&water, var, t, range, min);
        assert!(turn > 1.001 * range.0 && turn < 0.999 * range.1, "{var:?} turns at {turn}, an end of {range:?}");
        let beyond = water.state(dt(1.0005 * turn, t)).unwrap();
        let Err(Error::Ambiguous { roots }) = water.state(t_x(var, &beyond)) else { panic!("{var:?} by {turn}") };
        let near: Vec<f64> = roots.as_slice().iter().copied().filter(|r| (r / turn - 1.0).abs() < 2e-3).collect();
        let at = |r: &f64| (r / (1.0005 * turn) - 1.0).abs() < 1e-9;
        assert!(near.len() == 2 && near.iter().any(at), "{var:?}: {roots:?} by the turn at {turn}");
    }
}

/// M7.5: a saturated liquid's x is the dome's end and its single phase's, and the root both find is one root (they
/// agree to 1e-14 in ln ρ): HT of Water's saturated liquid at 20 temperatures from 300 K to 490 K is that state.
/// Above about 510 K its liquid's h falls with p at first (Tα > 1), and HT of h′ has a second, denser root.
#[cfg(feature = "fluids-all")]
#[test]
fn t_x_of_a_saturated_liquid_is_one_root() {
    let water = embedded("Water");
    for k in 0..20 {
        let side = water.state(q_t(0.0, 300.0 + 10.0 * f64::from(k))).unwrap();
        let back = water.state(t_x(Caloric::H, &side));
        let same = |s: &State| (s.rho(Basis::Molar) / side.rho(Basis::Molar) - 1.0).abs() < 1e-9;
        assert!(back.as_ref().is_ok_and(same), "{side:?}: {back:?}");
    }
}

/// M7.5 (D6, user decision 6): T+X checks T against the domain under `Enforce`, not under `Extrapolate`. Water's
/// HT, ST and TU at 2100 K, above its Tmax, are `AboveMaxTemperature`, and under `Extrapolate` the state, flagged.
#[cfg(feature = "fluids-all")]
#[test]
fn t_x_checks_t_against_the_domain() {
    let water = embedded("Water");
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let state = water.flash(dt(100.0, 2100.0), &extrapolate).unwrap();
    for var in [Caloric::H, Caloric::S, Caloric::U] {
        let input = t_x(var, &state);
        let refused = water.state(input);
        assert!(matches!(refused, Err(Error::Domain(DomainError::AboveMaxTemperature { .. }))), "{refused:?}");
        let back = water.flash(input, &extrapolate.with_roots(RootPolicy::Nearest(100.0))).unwrap();
        assert!(back.is_extrapolated() && (back.rho(Basis::Molar) / 100.0 - 1.0).abs() < 1e-9, "{var:?}: {back:?}");
    }
}

/// M7.5 (ROT-070): an imposed single phase holds along the whole isotherm, so the dome's root is not one. Water's
/// HT at 300 K for the h of its liquid at 10 MPa is `Ambiguous` between that liquid and a two-phase state without a
/// hint; under an imposed `Liquid` its roots are the EOS's along the whole isotherm, through its van der Waals
/// loop: three, the liquid among them and the two-phase state's density not.
#[cfg(feature = "fluids-all")]
#[test]
fn t_x_under_an_imposed_phase_skips_the_dome() {
    let water = embedded("Water");
    let liquid = water.state(pt(10e6, 300.0)).unwrap();
    let input = t_x(Caloric::H, &liquid);
    let roots = |opts: FlashOptions| match water.flash(input, &opts) {
        Err(Error::Ambiguous { roots }) => roots.as_slice().to_vec(),
        other => panic!("{other:?}"),
    };
    let (free, imposed) = (roots(FlashOptions::new()), roots(FlashOptions::new().with_phase(Phase::Liquid)));
    let has = |roots: &[f64], rho: f64| roots.iter().any(|r| (r / rho - 1.0).abs() < 1e-9);
    let &[two_phase, rho] = free.as_slice() else { panic!("{free:?}") };
    assert!((rho / liquid.rho(Basis::Molar) - 1.0).abs() < 1e-9, "{free:?}");
    assert!(imposed.len() == 3 && has(&imposed, rho) && !has(&imposed, two_phase), "{imposed:?}");
}

/// M7.5: where a pseudo-pure fluid's VLE fails just below its published Tc, above its EOS's own (Air from about
/// 131.9 K to 132.4 K), T+X runs on one stretch without a dome, as DT's single phase: HT of Air's gas at 100 mol/m³
/// and 132.2 K returns it.
#[cfg(feature = "fluids-all")]
#[test]
fn t_x_of_a_pseudo_pure_fluid_where_its_vle_fails() {
    let air = embedded("Air");
    let gas = air.state(dt(100.0, 132.2)).unwrap();
    let back = air.flash(t_x(Caloric::H, &gas), &FlashOptions::new().with_roots(RootPolicy::Nearest(100.0)));
    assert!(back.as_ref().is_ok_and(|s| (s.rho(Basis::Molar) / 100.0 - 1.0).abs() < 1e-9), "{back:?}");
}

/// M7.5: a pure fluid's phase comes from its curve, not a pseudo-pure fluid's density bands: Water at 0.999 of ρ′
/// at 400 K, inside the bands' liquid strip, is two-phase.
#[cfg(feature = "fluids-all")]
#[test]
fn a_pure_fluid_inside_its_dome_by_rho_l_is_two_phase() {
    let water = embedded("Water");
    let liquid = record_of("Water").superancillary_curve().unwrap().at_t(400.0).unwrap().bubble.rho;
    let state = water.state(dt(0.999 * liquid, 400.0)).unwrap();
    assert!(state.phase() == Phase::TwoPhase && state.quality().is_some_and(|q| q > 0.0 && q < 1e-3), "{state:?}");
}

/// M7.5: T+X's grid has 8 nodes per decade of ρ and at least 16 cells, evenly spaced from one end to the other.
#[test]
fn t_x_grids_have_eight_nodes_per_decade() {
    let ln10 = core::f64::consts::LN_10;
    let grids = [(0.0, 2.6 * ln10, 21), (2.0, 2.0 + 0.5 * ln10, 16), (-30.0, -30.0 + 14.9 * ln10, 120)];
    for (from, to, cells) in grids {
        let nodes: Vec<f64> = tx_nodes(from, to).collect();
        assert_eq!((nodes.len(), nodes.first(), nodes.last()), (cells + 1, Some(&from), Some(&to)));
        let step = (to - from) / cells as f64;
        let even = |w: &[f64]| matches!(w, [a, b] if ((b - a) / step - 1.0).abs() < 1e-9);
        assert!(nodes.windows(2).all(even), "{nodes:?}");
    }
}

/// M7.5: a pseudo-pure fluid's ancillaries are its phase rule only within their fitted range (D6): below it, under
/// `Extrapolate`, R410A's gas at 1 mol/m³ and 190 K (its curve starts at 200 K) is the EOS's, flagged.
#[cfg(feature = "fluids-all")]
#[test]
fn a_pseudo_pure_fluid_below_its_curve_extrapolates() {
    let r410a = embedded("R410A");
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let state = r410a.flash(dt(1.0, 190.0), &extrapolate);
    assert!(state.as_ref().is_ok_and(|s| s.phase() == Phase::Gas && s.is_extrapolated()), "{state:?}");
}
