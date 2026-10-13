//! `Prop`'s bound on a caloric output of the EOS, with `Term` carried through its relation (VERIFICATION.md §5), shared
//! by the saturation and reference-state tests. A test binary includes it with `#[path = "common/caloric.rs"] mod
//! caloric;`.

use phasekit_core::internal::FluidRecord;
use phasekit_core::{Fluid, Order};
use phasekit_verify::eos::{Majorants, carried};
use phasekit_verify::term::IdealScale;
use phasekit_verify::{ToleranceClass, Window};

/// `Prop`'s bound (VERIFICATION.md §5) on an h (`entropy` false) or s near `want` at (T, ρ), with `Term` carried
/// through the relation.
pub fn eos_bound(
    fluid: &Fluid,
    (record, ideal): (&FluidRecord, &IdealScale),
    t: f64,
    rho: f64,
    entropy: bool,
    want: f64,
) -> f64 {
    let (eos, r) = (fluid.model().helmholtz().unwrap(), record.eos.gas_constant);
    let critical = fluid.model().critical_point().unwrap();
    let bundle = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
    let majorants = Majorants::at(record, ideal, t, rho);
    let relation = if entropy { "smolar" } else { "hmolar" };
    let spread = carried(relation, &bundle, &majorants, r, t, rho, fluid.info().molar_mass());
    let scale = want.abs().max(if entropy { r } else { r * t });
    ToleranceClass::Prop.bound_carried(scale, Window::at(t, rho, critical.t, critical.rho), spread).unwrap()
}
