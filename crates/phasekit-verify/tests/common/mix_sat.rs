//! A mixture's bubble and dew points (PLAN.md M13.9): the oracle's `mix_sat` rows, the mixtures they belong to, and
//! each phase's fugacities at the state's pressure.

#![allow(dead_code, reason = "each test binary that includes the rows uses its part of them")]

use phasekit_core::{DataSet, Fluid, Order, Registry, math};
use phasekit_mix::{Mixture, MixtureModel, Saturation, Side, Var};
use phasekit_verify::{Fixture, fixture};

/// One of `mix_sat`'s bubble or dew points.
pub struct SatRow {
    /// The binary, "A&B", or the predefined mixture, "X.mix".
    pub label: String,
    /// The first component's mole fraction (NaN for a predefined mixture).
    pub z1: f64,
    /// Whether T is given (QT), else p (PQ).
    pub given_t: bool,
    /// Q: 0 at a bubble point, 1 at a dew point.
    pub q: f64,
    /// T (K) and p (Pa), one of them the input.
    pub t: f64,
    pub p: f64,
    /// The liquid's and the vapour's molar densities, and their first fractions (NaN for a predefined mixture).
    pub rho_liquid: f64,
    pub rho_vapour: f64,
    pub x1: f64,
    pub y1: f64,
}

impl SatRow {
    /// The side the row's Q names.
    pub fn side(&self) -> Side {
        if self.q == 0.0 { Side::Bubble } else { Side::Dew }
    }
}

/// Every row the oracle solved (`status` ok), in the file's order.
pub fn sat_rows() -> Vec<SatRow> {
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_sat.csv");
    let table = Fixture::parse(path, text).unwrap();
    (0..table.rows().len())
        .filter(|&row| table.printed(row, "status") == Some("ok"))
        .map(|row| {
            let value = |column: &str| table.value(row, column).unwrap();
            SatRow {
                label: table.printed(row, "mixture").unwrap().to_string(),
                z1: value("z1"),
                given_t: table.printed(row, "pair") == Some("QT"),
                q: value("Q"),
                t: value("T"),
                p: value("p"),
                rho_liquid: value("rho_liquid"),
                rho_vapour: value("rho_vapour"),
                x1: value("x1"),
                y1: value("y1"),
            }
        })
        .collect()
}

/// The mixture a row names on `Parity`, the oracle's data: a binary at z1, or a predefined mixture at its library's
/// mole fractions.
pub fn row_fluid(registry: &Registry, label: &str, z1: f64) -> Fluid {
    if let Some(name) = label.strip_suffix(".mix") {
        return phasekit_mix::predefined(registry, name, DataSet::Parity).unwrap();
    }
    let names: Vec<&str> = label.split('&').collect();
    phasekit_mix::mixture(registry, &[(names[0], z1), (names[1], 1.0 - z1)], DataSet::Parity).unwrap()
}

/// The mixture model behind `fluid`.
pub fn mixture(fluid: &Fluid) -> &Mixture {
    (fluid.model() as &dyn std::any::Any).downcast_ref::<Mixture>().unwrap()
}

/// A saturation point's liquid and vapour: (fractions, density) each. The feed's fractions are divided by their sum, as
/// the solve evaluated them: a dense liquid's ln φ moves by 1e-12 when its fractions move by an ulp (R418A's bubble point
/// at 258.27 K, whose z sums to 1 within an ulp).
pub fn phases(mixture: &Mixture, side: Side, s: &Saturation) -> ((Vec<f64>, f64), (Vec<f64>, f64)) {
    let z = mixture.composition();
    let total: f64 = z.iter().sum();
    let feed = (z.iter().map(|z| z / total).collect(), s.rho);
    let incipient = (s.incipient.clone(), s.rho_incipient);
    if side == Side::Bubble { (feed, incipient) } else { (incipient, feed) }
}

/// ln φ_i of mole fractions `x` at (T, ρ) at the pressure `p` the phase is at: ∂Ψ/∂n_i − ln(p/(ρRT)).
pub fn ln_phi(mixture: &Mixture, t: f64, p: f64, rho: f64, x: &[f64]) -> Vec<f64> {
    let c = mixture.model().composition(t, rho, x, Order::One).unwrap();
    let ln_z = math::ln(p / (rho * mixture.model().gas_constant() * t));
    (0..x.len()).map(|i| c.psi(&[Var::N(i)]) - ln_z).collect()
}

/// ln f_i^V − ln f_i^L of a liquid and a vapour at (T, p), each (fractions, density), and the bound `Identity` puts on
/// each (1e-12 of its terms' largest, at least 1).
pub fn fugacity_differences(
    mixture: &Mixture,
    t: f64,
    p: f64,
    (x, rho_l): (&[f64], f64),
    (y, rho_v): (&[f64], f64),
) -> Vec<(f64, f64)> {
    let (phi_l, phi_v) = (ln_phi(mixture, t, p, rho_l, x), ln_phi(mixture, t, p, rho_v, y));
    (0..x.len())
        .map(|i| {
            let terms = [math::ln(y[i]), phi_v[i], math::ln(x[i]), phi_l[i]];
            let bound = phasekit_verify::ToleranceClass::Identity
                .bound(terms.iter().fold(1.0_f64, |m, t| m.max(t.abs())))
                .unwrap();
            (terms[0] + terms[1] - terms[2] - terms[3], bound)
        })
        .collect()
}
