//! The conformance kit every model family inherits (Extensible graft): a new family dev-depends on this
//! crate and runs these checks unchanged. Identity-based, so they need no oracle.

use phasekit_core::batch::{self, BatchRequest, ExecPolicy, Status};
use phasekit_core::{Basis, Error, Fluid, Gauge, HelmholtzModel, Input, Order};

/// A failed check: which quantity, at which point, got vs expected.
#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    /// What was compared.
    pub what: &'static str,
    /// Index or point.
    pub at: usize,
    /// Value under test.
    pub got: f64,
    /// Expected value.
    pub want: f64,
}

/// Analytic `A10`, `A01` of the residual against central differences in the DIMENSIONAL variables
/// (`A10 = −T ∂α/∂T`, `A01 = ρ ∂α/∂ρ`), so a family's reducing choices cannot hide an error.
pub fn fd_first_order<M: HelmholtzModel + ?Sized>(
    m: &M,
    t: f64,
    rho: f64,
    rel_step: f64,
    tol: f64,
) -> Result<(), Mismatch> {
    let a = |t, rho| m.residual(t, rho, Order::One).get(0, 0).unwrap_or(f64::NAN);
    let d = m.residual(t, rho, Order::One);
    let (ht, hr) = (t * rel_step, rho * rel_step);
    let fd10 = -t * (a(t + ht, rho) - a(t - ht, rho)) / (2.0 * ht);
    let fd01 = rho * (a(t, rho + hr) - a(t, rho - hr)) / (2.0 * hr);
    for (what, got, want) in [("A10", d.get(1, 0), fd10), ("A01", d.get(0, 1), fd01)] {
        let got = got.unwrap_or(f64::NAN);
        let within = (got - want).abs() <= tol * want.abs().max(1e-300); // false for NaN
        if !within {
            return Err(Mismatch { what, at: 0, got, want });
        }
    }
    Ok(())
}

/// A gauge shifts h by exactly `dh` and s by exactly `ds` (molar) and changes nothing else (map 15 §8).
pub fn gauge_invariance(fluid: &Fluid, gauge: Gauge, inputs: &[Input]) -> Result<(), Mismatch> {
    let shifted = fluid.with_gauge(gauge);
    for (i, input) in inputs.iter().enumerate() {
        let (a, b) = match (fluid.state(*input), shifted.state(*input)) {
            (Ok(a), Ok(b)) => (a, b),
            _ => {
                return Err(Mismatch { what: "flash", at: i, got: f64::NAN, want: 0.0 });
            }
        };
        let dh = b.h(Basis::Molar) - a.h(Basis::Molar) - (gauge.dh() - fluid.gauge().dh());
        let ds = b.s(Basis::Molar) - a.s(Basis::Molar) - (gauge.ds() - fluid.gauge().ds());
        for (what, err, scale) in
            [("h", dh, a.h(Basis::Molar)), ("s", ds, a.s(Basis::Molar)), ("T", b.t() - a.t(), a.t())]
        {
            if err.abs() > 1e-12 * scale.abs().max(1.0) {
                return Err(Mismatch { what, at: i, got: err, want: 0.0 });
            }
        }
    }
    Ok(())
}

/// Every executor must reproduce `Reference` bitwise, values and statuses (K17).
pub fn policy_equivalence(fluid: &Fluid, req: &BatchRequest<'_>, candidate: ExecPolicy) -> Result<(), Mismatch> {
    let shape = Mismatch { what: "batch shape", at: 0, got: f64::NAN, want: 0.0 };
    let cells = req.cells().ok_or(shape.clone())?;
    let run = |exec| -> Result<(Vec<f64>, Vec<Status>), Error> {
        let (mut out, mut status) = (vec![0.0; cells], vec![Status::Ok; cells]);
        batch::evaluate(fluid, &req.with_exec(exec), &mut out, &mut status)?;
        Ok((out, status))
    };
    let (want, want_s) = run(ExecPolicy::Reference).map_err(|_| shape.clone())?;
    let (got, got_s) = run(candidate).map_err(|_| shape)?;
    let mut diffs = got.iter().zip(&want).zip(got_s.iter().zip(&want_s)).enumerate();
    match diffs.find(|(_, ((g, w), (gs, ws)))| g.to_bits() != w.to_bits() || gs != ws) {
        Some((at, ((&got, &want), _))) => Err(Mismatch { what: "cell", at, got, want }),
        None => Ok(()),
    }
}
