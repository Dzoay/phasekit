//! The Q-pairs QS, HQ and DQ on the saturation curve (PLAN.md M7.7).

use super::*;

/// The Q-pair input of `var` for `state` at its quality `q` (molar).
#[cfg(feature = "fluids-all")]
fn q_x(var: QVar, q: f64, state: &State) -> Input {
    let (q, m) = (Quality::new(q).unwrap(), Basis::Molar);
    match var {
        QVar::H => Input::hq(Enthalpy::molar(state.h(m)).unwrap(), q),
        QVar::S => Input::qs(q, Entropy::molar(state.s(m)).unwrap()),
        QVar::V => Input::dq(Density::molar(state.rho(m)).unwrap(), q),
    }
}

/// The temperatures of a Q-pair's roots, `Strict`'s `Ambiguous` list.
#[cfg(feature = "fluids-all")]
fn roots_of(fluid: &Fluid, input: Input) -> Vec<f64> {
    match fluid.state(input) {
        Err(Error::Ambiguous { roots }) => roots.as_slice().to_vec(),
        other => panic!("{input:?}: {other:?}"),
    }
}

/// map 03 §6 (#2773, ROT-071): Water's h″ peaks near 507 K, so HQ at Q = 1 for h″(400 K) has two roots; CoolProp's
/// default throws, listing 400 K and 587.912 K. Here `Strict` returns both temperatures (400 K within 1e-9, the
/// other within 1e-3 K of CoolProp's printed one), `Nearest` takes each, and each state's h is the input's.
#[cfg(feature = "fluids-all")]
#[test]
fn hq_water_400k_lists_both_roots() {
    let water = embedded("Water");
    let vapour = water.state(q_t(1.0, 400.0)).unwrap();
    let input = q_x(QVar::H, 1.0, &vapour);
    let roots = roots_of(&water, input);
    let &[low, high] = roots.as_slice() else { panic!("{roots:?}") };
    assert!((low / 400.0 - 1.0).abs() < 1e-9 && (high - 587.912).abs() < 1e-3, "{roots:?}");
    for t in [low, high] {
        let state = water.flash(input, &FlashOptions::new().with_roots(RootPolicy::Nearest(t))).unwrap();
        assert!(state.t() == t && (state.h(Basis::Molar) / vapour.h(Basis::Molar) - 1.0).abs() < 1e-12, "{state:?}");
        assert_eq!(state.path().strategy, Strategy::Saturation);
    }
}

/// map 03 §6: R1234ze(E)'s s″ turns twice, near 271 K and 338 K, so QS at Q = 1 for s″(275.6 K) has three roots;
/// CoolProp's error lists 267.297, 275.6 and 363.697 K. Here `Strict` returns the three (275.6 K within 1e-9, the
/// others within 1e-3 K of CoolProp's printed values).
#[cfg(feature = "fluids-all")]
#[test]
fn qs_r1234ze_e_vapour_has_three_roots() {
    let fluid = embedded("R1234ze(E)");
    let vapour = fluid.state(q_t(1.0, 275.6)).unwrap();
    let roots = roots_of(&fluid, q_x(QVar::S, 1.0, &vapour));
    let &[a, b, c] = roots.as_slice() else { panic!("{roots:?}") };
    assert!((a - 267.297).abs() < 1e-3 && (b / 275.6 - 1.0).abs() < 1e-9 && (c - 363.697).abs() < 1e-3, "{roots:?}");
}

/// map 03 §6: CoolProp's DQ brackets T only up to Tc − 0.1 K, so its DQ of Water at Tc − 0.05 K fails at Q = 0.5
/// ("do not bracket the root"; Q = 0 and 1 take its saturated-density path). Here DQ at Q = 0, 0.5 and 1 returns
/// that T within 1e-9, the critical point the superancillary's top.
#[cfg(feature = "fluids-all")]
#[test]
fn dq_works_near_tc() {
    let water = embedded("Water");
    let t = record_of("Water").superancillary_curve().unwrap().t_range().1 - 0.05;
    for q in [0.0, 0.5, 1.0] {
        let truth = water.state(q_t(q, t)).unwrap();
        let back = water.state(q_x(QVar::V, q, &truth)).unwrap();
        assert!((back.t() / t - 1.0).abs() < 1e-9 && back.quality() == Some(q), "Q = {q}: {back:?}");
    }
}

/// map 03 §3.1: CoolProp's HQ accepts only Q = 1 and its QS no fractional Q. Here every Q-pair accepts every Q: for
/// Water, R134a and n-Propane at 0.6 and 0.95 of the critical temperature and Q = 0, 0.25, 0.5, 0.75 and 1, QS, HQ
/// and DQ of the QT state return it (T within 1e-9; `Nearest` its T where the pair has several roots).
#[cfg(feature = "fluids-all")]
#[test]
fn q_pairs_are_symmetric_in_q() {
    for name in ["Water", "R134a", "n-Propane"] {
        let fluid = embedded(name);
        let t_c = record_of(name).superancillary_curve().unwrap().t_range().1;
        for (t, q) in [0.6, 0.95].into_iter().flat_map(|x| [0.0, 0.25, 0.5, 0.75, 1.0].map(|q| (x * t_c, q))) {
            let truth = fluid.state(q_t(q, t)).unwrap();
            for var in [QVar::S, QVar::H, QVar::V] {
                let input = q_x(var, q, &truth);
                let back = fluid.flash(input, &FlashOptions::new().with_roots(RootPolicy::Nearest(t))).unwrap();
                assert!((back.t() / t - 1.0).abs() < 1e-9 && back.quality() == Some(q), "{name} {input:?}: {back:?}");
            }
        }
    }
}

/// M7.7: a Q-pair scans x(T) on the caloric curves (M5.2a), decoded from the blob with the record, and polishes its
/// roots on QT's state. The curves' lever rule gives h and s of Water's QT state within `CaloricFit` (2e-6 of the
/// value) and none for v, which comes from the superancillary's densities; on the state, h, s and v = 1/ρ are read
/// as they are.
#[cfg(feature = "fluids-all")]
#[test]
fn q_pairs_read_the_caloric_curves() {
    let (water, calorics) = (embedded("Water"), record_of("Water").calorics().unwrap());
    for (q, t) in [(0.3, 400.0), (0.7, 600.0)] {
        let (state, sides) = (water.state(q_t(q, t)).unwrap(), calorics.at(t).unwrap());
        let m = Basis::Molar;
        for (var, want) in [(QVar::H, state.h(m)), (QVar::S, state.s(m))] {
            let lever = var.lever(q, sides).unwrap();
            assert!((lever - want).abs() <= 2e-6 * want.abs(), "{var:?} at {t} K: {lever} against {want}");
            assert_eq!(var.of(&state), want);
        }
        assert_eq!(QVar::V.lever(q, sides), None);
        assert_eq!(QVar::V.of(&state), 1.0 / state.rho(m));
    }
}

/// M7.7: two roots inside one grid cell, either side of a turn of x(T), are both found: golden-section search puts
/// the turn between the cell's nodes, and it brackets each root. h″ peaks below the critical point, so HQ at Q = 1
/// for the h″ 2 mK below its peak has its two roots within 10 mK, in the grid cell that holds the peak; both are
/// returned, `Ambiguous`. For six fluids whose peak lies at least 0.05 K from a node, with peaks in either half of
/// their cells: a search that found the turn only from a neighbouring triple of nodes would miss some.
#[cfg(feature = "fluids-all")]
#[test]
fn two_roots_in_one_cell_are_both_found() {
    let mut checked = 0;
    for name in ["Water", "R134a", "n-Propane", "CarbonDioxide", "Ammonia", "Nitrogen"] {
        let fluid = embedded(name);
        let (lo, hi) = record_of(name).superancillary_curve().unwrap().t_range();
        let h_vapour = |t: f64| fluid.state(q_t(1.0, t)).unwrap().h(Basis::Molar);
        let highest = |ts: &mut dyn Iterator<Item = f64>| ts.max_by(|a, b| h_vapour(*a).total_cmp(&h_vapour(*b)));
        let coarse = highest(&mut (0..1000).map(|k| lo + (hi - 1.0 - lo) * f64::from(k) / 1000.0)).unwrap();
        let peak = highest(&mut (0..=2000).map(|k| coarse - 1.0 + f64::from(k) * 1e-3)).unwrap();
        let cell = (hi - lo) / f64::from(QX_CELLS);
        let a = lo + ((peak - lo) / cell).floor() * cell;
        if (peak - a).min(a + cell - peak) < 0.05 {
            continue;
        }
        // 2 mK below the peak (found to 1 mK): the turn must be placed within about 2 mK of it to bracket both.
        let t = peak - 2e-3;
        let roots = roots_of(&fluid, q_x(QVar::H, 1.0, &fluid.state(q_t(1.0, t)).unwrap()));
        let &[low, high] = roots.as_slice() else { panic!("{name}: {roots:?}") };
        let both = (low / t - 1.0).abs() < 1e-9 && high > peak && high < t + 1e-2;
        assert!(both, "{name}: {roots:?}, h″ peaking at {peak} K in the cell from {a} K");
        checked += 1;
    }
    assert!(checked >= 4, "{checked} fluids");
}

/// M7.7: a root found on the caloric curves is polished on the EOS: TOMS 748 in a bracket around it, widened
/// tenfold from 1e-9 of T until it holds a sign change. A residual whose root lies 3e-7 of T away is polished onto
/// it; one whose root lies beyond 1e-4 of T leaves the curves' root as it is.
#[test]
fn polish_moves_a_root_onto_the_eos() {
    for (offset, want) in [(3e-7, 400.0 * (1.0 + 3e-7)), (-3e-7, 400.0 * (1.0 - 3e-7)), (1e-3, 400.0)] {
        let root = 400.0 * (1.0 + offset);
        let got = polish(&|t: f64| Ok(t - root), 400.0).unwrap();
        assert!((got / want - 1.0).abs() < 1e-12, "{offset}: {got}");
    }
}

/// M7.7: a pseudo-pure fluid has no caloric curves, so its Q-pairs scan QT's own state: Air's HQ and DQ at Q = 1
/// and QS at Q = 0 at 100 K return that T, `Nearest` where h″ turns (its QT takes only Q = 0 and 1, D4).
#[cfg(feature = "fluids-all")]
#[test]
fn pseudo_pure_q_pairs_scan_qt() {
    let air = embedded("Air");
    for (var, q) in [(QVar::H, 1.0), (QVar::V, 1.0), (QVar::S, 0.0)] {
        let truth = air.state(q_t(q, 100.0)).unwrap();
        let nearest = FlashOptions::new().with_roots(RootPolicy::Nearest(100.0));
        let back = air.flash(q_x(var, q, &truth), &nearest).unwrap();
        assert!((back.t() / 100.0 - 1.0).abs() < 1e-9 && back.quality() == Some(q), "{var:?}: {back:?}");
    }
}

/// M7.7: a root exactly on a grid node closes the brackets either side of it and is one root. DQ at Q = 0 for
/// Water's ρ′ at a node's temperature makes the scan's residual exactly 0 there (v′ from the same superancillary
/// densities): one state at that T, not `Ambiguous`.
#[cfg(feature = "fluids-all")]
#[test]
fn a_root_on_a_grid_node_is_one_root() {
    let water = embedded("Water");
    let curve = record_of("Water").superancillary_curve().unwrap();
    let (lo, hi) = curve.t_range();
    let t = lo + (hi - lo) * f64::from(QX_CELLS / 2) / f64::from(QX_CELLS);
    let liquid = Density::molar(curve.at_t(t).unwrap().bubble.rho).unwrap();
    let state = water.state(Input::dq(liquid, Quality::new(0.0).unwrap())).unwrap();
    assert!((state.t() / t - 1.0).abs() < 1e-9 && state.quality() == Some(0.0), "{state:?}");
}

/// M7.7: an x that no saturated state at Q holds is `InvalidInput`, naming the input: HQ at Q = 1 above Water's
/// largest h″, QS at Q = 0 above its s at the critical point, DQ at Q = 0 above its largest ρ′.
#[cfg(feature = "fluids-all")]
#[test]
fn a_q_pair_without_a_root_is_invalid_input() {
    let water = embedded("Water");
    let (one, zero) = (Quality::new(1.0).unwrap(), Quality::new(0.0).unwrap());
    let cases = [
        (Input::hq(Enthalpy::molar(60_000.0).unwrap(), one), Quantity::enthalpy(Basis::Molar), 60_000.0),
        (Input::qs(zero, Entropy::molar(500.0).unwrap()), Quantity::entropy(Basis::Molar), 500.0),
        (Input::dq(Density::molar(60_000.0).unwrap(), zero), Quantity::density(Basis::Molar), 60_000.0),
    ];
    for (input, quantity, value) in cases {
        assert_eq!(water.state(input), Err(Error::InvalidInput { quantity, value }), "{input:?}");
    }
}

/// M7.9: a Q-pair's grid over [100 K, 500 K] is 200 even cells of 2 K, then its last cell halved 30 times towards
/// 500 K (499 K, 499.5 K, … to 2e-9 K of it), then 500 K.
#[test]
fn q_pair_grids_halve_their_last_cell() {
    let nodes: Vec<f64> = qx_nodes(100.0, 500.0).collect();
    assert_eq!(nodes.len(), 200 + 30 + 1);
    let (cells, top) = nodes.split_at(200);
    assert!(cells.iter().zip(0..).all(|(&t, k)| t == 100.0 + 2.0 * f64::from(k)), "{cells:?}");
    let halved = (1..=30).map(|k| 500.0 - math::powi(2.0, 1 - k)).chain([500.0]);
    assert!(top.iter().zip(halved).all(|(&t, want)| t == want), "{top:?}");
}
