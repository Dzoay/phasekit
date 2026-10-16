//! A GERG-2008 component's own equation of state (Kunz and Wagner 2012, eqs. 7.5 and 7.7; GERG TM15 Tables A3.1-A3.4
//! for the 18 GERG-2004 components): the residual Σ n δ^d τ^t exp(−δ^c), the exponential absent where c = 0, in one
//! kernel with the departure functions (an Exponential-kind function, M13.3), and the ideal gas
//!
//! α⁰_oi = ln δ + (R*/R)[n₁ + n₂ τ + n₃ ln τ + Σ n_k ln|sinh(ϑ_k τ)| − Σ n_k ln cosh(ϑ_k τ)],
//!
//! its own closed term kind, not a generalized Planck-Einstein one (ROT-205). AGA8 writes it in T and D (mol/l):
//! α⁰_oi = ln D + N₁ + N₂/T − N₃ ln T + Σ N_k ln|sinh(θ_k/T)| − Σ N_k ln cosh(θ_k/T), θ_k in K and the N its n
//! times R*/R, R* = 8.31451 J/(mol·K) the gas constant the ideal-gas equations were fitted with. n₁ and n₂ fix
//! h = s = 0 for the ideal gas at T₀ = 298.15 K and p₀ = 101.325 kPa; AGA8 prints them to ten digits and here they are
//! computed exactly, as AGA8's own commented-out code computes them (`GERG2008.cpp`, `SetupGERG`, after `return`).

use phasekit_core::{Derivs, Fluid, Jet4, Order, Real, Reducing, Virials, math};

use crate::data::{DepartureData, Kind};
use crate::departure;
use crate::gerg::{Component, R_GERG};

/// The gas constant GERG's ideal-gas equations were fitted with, J/(mol·K).
const R_STAR: f64 = 8.314_51;

/// The ideal gas's reference state: h = s = 0 at T₀ (K) and p₀ (kPa).
const T0: f64 = 298.15;
const P0: f64 = 101.325;

/// A hyperbolic ideal-gas term: N_k, θ_k (K) and whether it is a sinh term (else a cosh one).
type Hyperbolic = (f64, f64, bool);

/// One component's equation.
#[derive(Debug)]
pub(crate) struct Pure {
    residual: DepartureData,
    t_c: f64,
    /// ρ_c, mol/m³.
    rho_c: f64,
    /// α⁰'s τ-part in u = 1/T: the constant (with ln D_c, so that ln δ + it is ln D), N₂ and N₃.
    ideal: [f64; 3],
    /// N_k, θ_k (K) and whether the term is a sinh one, for each hyperbolic term present.
    hyperbolic: Box<[Hyperbolic]>,
    /// The fluid whose density bound brackets the density solve (the registry's reference equation of the same
    /// substance: GERG states none of its own).
    bound: Fluid,
}

impl Pure {
    /// `component`'s equation, its density bound `bound`'s.
    pub(crate) fn new(component: &'static Component, bound: Fluid) -> Pure {
        let residual = DepartureData {
            name: component.name,
            aliases: &[],
            source: "Kunz-JCED-2012",
            kind: Kind::Exponential,
            power: component.residual.len(),
            terms: component.residual,
        };
        let (ideal, hyperbolic) = ideal(component);
        Pure { residual, t_c: component.t_c, rho_c: 1e3 * component.rho_c, ideal, hyperbolic, bound }
    }

    /// α⁰'s τ-part (without ln δ) at u = 1/T, on any `Real`.
    fn tau_part<R: Real>(&self, u: R) -> R {
        let c = |x: f64| R::from_f64(x);
        let [constant, n2, n3] = self.ideal;
        let mut sum = c(constant) + c(n2) * u + c(n3) * u.ln();
        for &(n, theta, sinh) in self.hyperbolic.iter() {
            let x = c(theta) * u;
            sum = if sinh { sum + c(n) * x.sinh().ln() } else { sum - c(n) * x.cosh().ln() };
        }
        sum
    }
}

/// `component`'s hyperbolic terms (n_k, θ_k, sinh?) before AGA8's R*/R scaling.
fn hyperbolic(component: &Component) -> Vec<Hyperbolic> {
    let [_, _, _, ref coefficients @ ..] = component.n0;
    coefficients
        .iter()
        .zip(component.theta)
        .enumerate()
        .filter(|(_, (_, theta))| *theta > 0.0)
        .map(|(k, (n, theta))| (*n, theta, k % 2 == 0))
        .collect()
}

/// `component`'s n₁ and n₂ as AGA8 writes them (`n0i[i][1]` and `n0i[i][2]`), computed exactly for h = s = 0 at
/// (T₀, p₀) as its commented-out code computes them: AGA8 prints them to ten digits.
pub(crate) fn aga8_constants(component: &Component) -> [f64; 2] {
    let (mut n1, mut n2) = (0.0, 0.0);
    for (n, theta, sinh) in hyperbolic(component) {
        let x = theta / T0;
        let (sh, ch) = (math::sinh(x), math::cosh(x));
        if sinh {
            (n1, n2) = (n1 - n * math::ln(sh), n2 - n * theta * ch / sh);
        } else {
            (n1, n2) = (n1 + n * math::ln(ch), n2 + n * theta * sh / ch);
        }
    }
    let n3 = component.n0[2] - 1.0;
    // AGA8 adds T₀ to its printed n₂ before scaling, then subtracts it.
    [n1 - n2 / T0 + n3 * (1.0 + math::ln(T0)), n2 - n3 * T0 - T0]
}

/// `component`'s α⁰ τ-part in u = 1/T ([`Pure::ideal`]'s constant, N₂, N₃) and its hyperbolic terms (N_k, θ_k,
/// sinh?): AGA8's constants for h = s = 0 at (T₀, p₀), computed, then its R*/R scaling (`SetupGERG`'s last loop).
fn ideal(component: &Component) -> ([f64; 3], Box<[Hyperbolic]>) {
    let ratio = R_STAR / R_GERG;
    let [n1, n2] = aga8_constants(component);
    let n3 = component.n0[2] - 1.0;
    let d0 = P0 / R_GERG / T0;
    let constant = ratio * n1 - math::ln(d0) + math::ln(component.rho_c);
    let constants = [constant, ratio * (n2 + T0) - T0, ratio * n3];
    (constants, hyperbolic(component).iter().map(|&(n, theta, sinh)| (ratio * n, theta, sinh)).collect())
}

impl phasekit_core::HelmholtzModel for Pure {
    fn gas_constant(&self) -> f64 {
        R_GERG
    }

    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        departure::derivs(&self.residual, self.t_c / t, rho / self.rho_c, order)
    }

    /// The τ-part in u = 1/T: u^i ∂^i/∂u^i is τ^i ∂^i/∂τ^i, whatever T_r is.
    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
        let u = 1.0 / t;
        let jet = self.tau_part(Jet4::tau(u)).derivs(u, 1.0);
        let ln_delta = math::ln(rho / self.rho_c);
        Derivs::from_fn(order, |i, j| match (i, j) {
            (0, 0) => ln_delta + jet.get(0, 0).unwrap_or(f64::NAN),
            (0, j) => Derivs::IDEAL_DELTA.get(0, j).unwrap_or(0.0),
            (i, 0) => jet.get(i, 0).unwrap_or(f64::NAN),
            _ => 0.0,
        })
    }

    fn rho_max(&self, t: f64) -> f64 {
        self.bound.model().helmholtz().map_or(f64::NAN, |m| m.rho_max(t))
    }

    /// α^r = Σ a_k(τ) δ^k near δ = 0, from the residual's jet there: B = α^r_δ/ρ_c, C = α^r_δδ/ρ_c².
    fn zero_density(&self, t: f64) -> Option<Virials> {
        let jet = departure::jet(&self.residual, self.t_c / t, 0.0);
        let at = |i, j| jet.derivative(i, j).unwrap_or(f64::NAN);
        let dtau_dt = -self.t_c / (t * t);
        let (r1, r2) = (1.0 / self.rho_c, 1.0 / (self.rho_c * self.rho_c));
        Some(Virials {
            b: at(0, 1) * r1,
            c: at(0, 2) * r2,
            db_dt: at(1, 1) * dtau_dt * r1,
            dc_dt: at(1, 2) * dtau_dt * r2,
        })
    }

    fn reducing(&self) -> Option<Reducing> {
        Some(Reducing { t: self.t_c, rho: self.rho_c, p: None })
    }
}
