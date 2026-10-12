//! The ideal-gas kinds as datagen converts them (PLAN.md M4.2), on real data, against `Jet4` AD of their paper
//! formulas, orders 0-4.

use super::*;

/// The τ-derivatives `τ^k d^k α⁰/dτ^k`, k = 0..4, of `name`'s ideal terms that `keep` selects, compiled alone
/// (as datagen converted them) and evaluated at ρ = ρ_r, where the ln δ of α⁰ is 0.
fn ideal_part(records: &[(Source, FluidRecord)], name: &str, keep: fn(&IdealTerm) -> bool, t: f64) -> [f64; 5] {
    let record = &records.iter().find(|(_, r)| r.name == name).unwrap().1;
    let mut eos = record.eos.clone();
    eos.ideal.retain(keep);
    let (rho_r, limits) = (eos.rho_reducing, record.limits);
    let fluid = FluidRecord::new(name, record.molar_mass, record.source.clone(), eos, limits).compile().unwrap();
    let d = fluid.eos().ideal(t, rho_r, Order::Four);
    [0, 1, 2, 3, 4].map(|k| d.get(k, 0).unwrap())
}

/// `name`'s JSON α⁰ blocks of one type.
fn ideal_blocks<'a>(records: &'a [(Source, FluidRecord)], name: &str) -> (&'a [IdealBlock], f64, Limits) {
    let (s, r) = records.iter().find(|(_, r)| r.name == name).unwrap();
    (&s.fluid.eos[0].alpha0, r.eos.t_reducing, r.limits)
}

/// Four temperatures in the fluid's range and τ = T_r/T at each.
fn ideal_points(t_r: f64, limits: Limits) -> [(f64, f64); 4] {
    let ts = [limits.t_min(), t_r, (1.5 * t_r).min(limits.t_max()), limits.t_max()];
    ts.map(|t| (t, t_r / t))
}

/// `got` within 1e-12 of `scale[k]` from the jet's `τ^k d^k/dτ^k`, orders `from..=4`; returns the orders compared.
fn assert_ideal(what: &str, got: [f64; 5], want: &Jet4, tau: f64, from: usize, scale: [f64; 5]) -> usize {
    let want = want.derivs(tau, 1.0);
    for k in from..=4 {
        let w = want.get(k, 0).unwrap();
        assert!((got[k] - w).abs() <= 1e-12 * scale[k].max(w.abs()), "{what}, τ = {tau}, order {k}: {} vs {w}", got[k]);
    }
    5 - from
}

/// α⁰ of `c_p⁰/R = c T^t` from its definition with T = tc/τ (CoolProp's CP0PolyT and CP0Constant, the block's own
/// `Tc` in place of T_r): `(1/T)∫_{T0}^{T} c T′^t dT′ − ∫_{T0}^{T} c T′^(t−1) dT′`.
fn cp0_alpha(c: f64, t: f64, tc: f64, t0: f64, tau: Jet4) -> Jet4 {
    let temp = Jet4::constant(tc) / tau;
    let inv = Jet4::constant(1.0) / temp;
    let log = (temp * (1.0 / t0)).ln();
    if t == 0.0 {
        (Jet4::constant(1.0) - inv * t0) * c - log * c
    } else if t == -1.0 {
        inv * log * c - (Jet4::constant(1.0 / t0) - inv) * c
    } else {
        let first = (temp.powf(t + 1.0) + -math::powf(t0, t + 1.0)) * inv * (c / (t + 1.0));
        first - (temp.powf(t) + -math::powf(t0, t)) * (c / t)
    }
}

/// PLAN.md M4.2 (map 02 §3.2): Nitrogen's `IdealGasHelmholtzPlanckEinsteinFunctionT` block, converted to θ = v/Tcrit,
/// against `Jet4` AD of `n ln(1 − e^(−vτ/Tcrit))`, orders 0-4.
#[test]
fn planck_einstein_function_t_matches_ad() {
    let records = records();
    let (blocks, t_r, limits) = ideal_blocks(&records, "Nitrogen");
    let [IdealBlock::PlanckEinsteinFunctionT { n, v, t_crit, .. }] =
        blocks.iter().filter(|b| matches!(b, IdealBlock::PlanckEinsteinFunctionT { .. })).collect::<Vec<_>>()[..]
    else {
        panic!("Nitrogen has one FunctionT block")
    };
    let mut checked = 0;
    for (t, tau) in ideal_points(t_r, limits) {
        let got = ideal_part(&records, "Nitrogen", |k| matches!(k, IdealTerm::PlanckEinstein { .. }), t);
        let x = Jet4::tau(tau);
        let want = n
            .iter()
            .zip(v)
            .fold(Jet4::constant(0.0), |sum, (n, v)| sum + (Jet4::constant(1.0) - (x * (-v / t_crit)).exp()).ln() * *n);
        let scale = [0, 1, 2, 3, 4]
            .map(|k| n.iter().zip(v).map(|(n, v)| n.abs() * math::powi(1.0 + v / t_crit * tau, k)).sum());
        checked += assert_ideal("Nitrogen FunctionT", got, &want, tau, 0, scale);
    }
    assert_eq!(checked, 4 * 5);
}

/// PLAN.md M4.2: Air's `IdealGasHelmholtzPlanckEinsteinGeneralized` term (θ = 87.31, c = 2/3, d = 1) against
/// `Jet4` AD of `n ln(c + d e^(θτ))`, orders 0-4, within 1e-12 of |n|(1 + |θτ|)^k.
#[test]
fn planck_einstein_generalized_matches_ad() {
    let records = records();
    let (blocks, t_r, limits) = ideal_blocks(&records, "Air");
    let [IdealBlock::PlanckEinsteinGeneralized { n, t: theta, c, d }] =
        blocks.iter().filter(|b| matches!(b, IdealBlock::PlanckEinsteinGeneralized { .. })).collect::<Vec<_>>()[..]
    else {
        panic!("Air has one generalized block")
    };
    let mut checked = 0;
    for (t, tau) in ideal_points(t_r, limits) {
        let got = ideal_part(&records, "Air", |k| matches!(k, IdealTerm::PlanckEinsteinGeneralized { .. }), t);
        let x = Jet4::tau(tau);
        // ln(c + d e^(θτ)) = θτ + ln(d + c e^(−θτ)): the same function, written so its AD neither overflows nor
        // underflows (at T_min, θτ ≈ 194 and 1/(c + d e^(θτ))⁴ is below the smallest double).
        let full = (x * theta[0] + ((x * -theta[0]).exp() * c[0] + d[0]).ln()) * n[0];
        let scale = [0, 1, 2, 3, 4].map(|k| n[0].abs() * math::powi(1.0 + (theta[0] * tau).abs(), k));
        checked += assert_ideal("Air generalized", got, &full, tau, 0, scale);
    }
    assert_eq!(checked, 4 * 5);
}

/// PLAN.md M4.2: CarbonDioxide's `IdealGasHelmholtzEnthalpyEntropyOffset` (IIR) against `Jet4` AD of `a1 + a2 τ`.
#[test]
fn enthalpy_entropy_offset_matches_ad() {
    let records = records();
    let (blocks, t_r, limits) = ideal_blocks(&records, "CarbonDioxide");
    let [IdealBlock::EnthalpyEntropyOffset { a1, a2, reference }] =
        blocks.iter().filter(|b| matches!(b, IdealBlock::EnthalpyEntropyOffset { .. })).collect::<Vec<_>>()[..]
    else {
        panic!("CarbonDioxide has one offset")
    };
    assert_eq!(reference, "IIR");
    for (t, tau) in ideal_points(t_r, limits) {
        let got = ideal_part(&records, "CarbonDioxide", |k| matches!(k, IdealTerm::Offset { .. }), t);
        let want = Jet4::tau(tau) * *a2 + *a1;
        assert_ideal("CO2 offset", got, &want, tau, 0, [a1.abs() + (a2 * tau).abs(), (a2 * tau).abs(), 0.0, 0.0, 0.0]);
    }
}

/// HFE143m's c_p⁰ blocks of one JSON type (CP0PolyT: t = 1, 2, 3; CP0Constant: t = 0), converted to c_p⁰ power
/// terms, against `Jet4` AD of the α⁰ their c_p⁰ defines, orders 0-4. Each term's summands are of order one here
/// (c T^t ≤ 10 for T ≤ T_max), so the floor of 1 is their scale.
fn hfe143m_cp0_matches_ad(constant: bool) -> usize {
    let records = records();
    let (blocks, t_r, limits) = ideal_blocks(&records, "HFE143m");
    let mut terms = Vec::new();
    for block in blocks {
        match block {
            IdealBlock::Cp0PolyT { c, t, tc, t0, .. } if !constant => {
                terms.extend(c.iter().zip(t).map(|(c, t)| (*c, *t, *tc, *t0)));
            }
            IdealBlock::Cp0Constant { cp_over_r, tc, t0 } if constant => terms.push((*cp_over_r, 0.0, *tc, *t0)),
            _ => {}
        }
    }
    assert_eq!(terms.len(), if constant { 1 } else { 3 });
    let keep: fn(&IdealTerm) -> bool = if constant {
        |k| matches!(k, IdealTerm::Cp0Power { t: 0.0, .. })
    } else {
        |k| matches!(k, IdealTerm::Cp0Power { t, .. } if *t != 0.0)
    };
    let mut checked = 0;
    for (t, tau) in ideal_points(t_r, limits) {
        let got = ideal_part(&records, "HFE143m", keep, t);
        let x = Jet4::tau(tau);
        let want = terms.iter().fold(Jet4::constant(0.0), |sum, &(c, e, tc, t0)| sum + cp0_alpha(c, e, tc, t0, x));
        checked += assert_ideal("HFE143m c_p⁰", got, &want, tau, 0, [1.0; 5]);
    }
    checked
}

/// PLAN.md M4.2: HFE143m's `IdealGasHelmholtzCP0PolyT` block (t = 1, 2, 3).
#[test]
fn cp0_poly_t_matches_ad() {
    assert_eq!(hfe143m_cp0_matches_ad(false), 4 * 5);
}

/// PLAN.md M4.2: HFE143m's `IdealGasHelmholtzCP0Constant` block.
#[test]
fn cp0_constant_matches_ad() {
    assert_eq!(hfe143m_cp0_matches_ad(true), 4 * 5);
}

/// PLAN.md M4.2, map 13 §8: n-Heptane's two `IdealGasHelmholtzCP0AlyLee` blocks, converted as CoolProp converts
/// them, reproduce the paper's `c_p⁰/R = A + B[(C/T)/sinh(C/T)]² + D[(E/T)/cosh(E/T)]²` (Aly & Lee 1981) as
/// −τ²∂²α⁰/∂τ², and its τ-derivatives through order 4 (orders 2-4 of α⁰; 0 and 1 hold integration constants).
#[test]
fn cp0_aly_lee_matches_ad() {
    let records = records();
    let (blocks, t_r, limits) = ideal_blocks(&records, "n-Heptane");
    let constants: Vec<[f64; 5]> = blocks
        .iter()
        .filter_map(|b| match b {
            IdealBlock::Cp0AlyLee { c, .. } => c.as_slice().try_into().ok(),
            _ => None,
        })
        .collect();
    assert_eq!(constants.len(), 2);
    let keep = |k: &IdealTerm| matches!(k, IdealTerm::Cp0Power { .. } | IdealTerm::PlanckEinsteinGeneralized { .. });
    for (t, tau) in ideal_points(t_r, limits) {
        let got = ideal_part(&records, "n-Heptane", keep, t);
        // c_p⁰/R(τ) with T = T_r/τ, then α⁰'' = −c_p⁰/(R τ²): its jet gives α⁰'', α⁰''' and α⁰''''.
        let x = Jet4::tau(tau);
        let temp = Jet4::constant(t_r) / x;
        let cp = constants.iter().fold(Jet4::constant(0.0), |sum, &[a, b, c, d, e]| {
            let (u, w) = (Jet4::constant(c) / temp, Jet4::constant(e) / temp);
            let sinh = u / u.sinh();
            let cosh = w / w.cosh();
            sum + sinh * sinh * b + cosh * cosh * d + a
        });
        let second = cp * -1.0 / (x * x);
        let want = [second.value(), second.derivative(1, 0).unwrap(), second.derivative(2, 0).unwrap()];
        for (k, w) in want.into_iter().enumerate() {
            let scaled = w * math::powi(tau, k as i32 + 2);
            let g = got[k + 2];
            assert!((g - scaled).abs() <= 1e-12 * scaled.abs().max(1.0), "τ = {tau}, order {}: {g} vs {scaled}", k + 2);
        }
    }
}
