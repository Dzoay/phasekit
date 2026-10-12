//! L5 transport: closed sum types of CoolProp's correlation forms (map 05 U2), materialised lazily per fluid
//! and written only against the public `State` API (materials S5). Crate-private: the forms are validated
//! data, like term kinds (§11). Coefficient fields arrive at M8.

use crate::error::Error;
use crate::fluid::Fluid;
use crate::math;
use crate::prop::Prop;

/// Viscosity forms.
#[derive(Clone, Debug)]
#[expect(dead_code, reason = "the M8 decoder builds Staged and IapwsWater and reads the ECS reference")]
pub(crate) enum ViscosityModel {
    /// Dilute + initial-density + residual stages, evaluated and tested stage by stage (map 05 U3).
    Staged,
    /// IAPWS 2008 water (map 05 U7).
    IapwsWater,
    /// Extended corresponding states (map 05 U8) against a reference fluid held strongly: the registry
    /// resolved it when the layer was built, so this `Fluid` works even after its registry is gone (S-05).
    Ecs {
        /// The reference fluid (Propane, R134a or Nitrogen in v8.0.0).
        reference: Fluid,
    },
}

/// Surface tension `σ(T) = Σ a_i (1 − T/Tc)^n_i` (108 fluids; map 05 U5).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SurfaceTension {
    /// Critical temperature the correlation was fitted with, K.
    t_c: f64,
    /// `(a_i, n_i)` pairs.
    terms: Box<[(f64, f64)]>,
}

impl SurfaceTension {
    /// σ in N/m; refuses T ≥ Tc instead of returning 0 or NaN.
    fn eval(&self, t: f64) -> Result<f64, Error> {
        if !(t > 0.0 && t < self.t_c) {
            return Err(Error::NoModel { prop: Prop::SurfaceTension });
        }
        let x = 1.0 - t / self.t_c;
        Ok(self.terms.iter().map(|&(a, n)| a * math::powf(x, n)).sum())
    }
}

/// The transport closure of one fluid, materialised on its first transport call, so thermo-only users
/// never decode it or load its ECS reference fluids (Lean fatal flaw; Kernel graft). Conductivity forms
/// (Staged, IapwsWater, Ecs) join at M8 beside viscosity.
#[derive(Clone, Debug, Default)]
pub(crate) struct TransportSet {
    #[expect(dead_code, reason = "read by the M8 evaluator")]
    pub(crate) viscosity: Option<ViscosityModel>,
    pub(crate) surface_tension: Option<SurfaceTension>,
}

impl TransportSet {
    /// σ(T) or `NoModel`.
    pub(crate) fn surface_tension(&self, t: f64) -> Result<f64, Error> {
        self.surface_tension.as_ref().ok_or(Error::NoModel { prop: Prop::SurfaceTension })?.eval(t)
    }
}
