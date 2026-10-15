//! A cubic fluid: the cubic's residual part on the ideal gas of the fluid it was built from (ARCHITECTURE.md §11;
//! map 06 U2, U3; user decision CB1).

use core::any::Any;
use std::sync::Arc;

use phasekit_core::{
    Constant, Constants, CriticalOrigin, CriticalPoint, DataSet, DataTerms, Derivs, Error, Fluid, FluidInfo,
    HelmholtzModel, ModelKey, Order, Prop, PureFluid, Reducing, Registry, SatAccuracy, Source, Virials,
};

use crate::{Alpha, CubicSaturation, Kind, R_CODATA, Residual};

/// A cubic's `HelmholtzModel`: α^r from [`Residual`] and α⁰ the source fluid's own, unchanged; one R, CoolProp's
/// `R_CODATA`, scales both (user decision CB1, DIV-0046).
#[derive(Debug)]
pub struct CubicModel {
    residual: Residual,
    source: Fluid,
}

impl CubicModel {
    /// The residual part.
    pub const fn residual_part(&self) -> &Residual {
        &self.residual
    }

    /// The fluid whose critical point, acentric factor and ideal gas the cubic takes.
    pub const fn source(&self) -> &Fluid {
        &self.source
    }
}

impl HelmholtzModel for CubicModel {
    fn gas_constant(&self) -> f64 {
        R_CODATA
    }

    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        self.residual.derivs(t, rho, order)
    }

    /// The source's α⁰ at the same (T, ρ): reducing-invariant `Derivs`, so its reducing state never meets the cubic's
    /// (map 06 C1, ROT-212).
    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
        match self.source.model().helmholtz() {
            Some(eos) => eos.ideal(t, rho, order),
            None => Derivs::from_fn(order, |_, _| f64::NAN),
        }
    }

    /// Just below the co-volume's pole at ρ = 1/b.
    fn rho_max(&self, _t: f64) -> f64 {
        (1.0 - 1e-6) * self.residual.rho_max()
    }

    fn zero_density(&self, t: f64) -> Option<Virials> {
        Some(self.residual.virials(t))
    }
}

impl Kind {
    /// The prefix of this kind's fluid names, CoolProp's for SRK and PR: `VDW`, `SRK` and `PR` (`PR::n-Propane`).
    pub const fn prefix(self) -> &'static str {
        match self {
            Kind::Vdw => "VDW",
            Kind::Srk => "SRK",
            Kind::Pr => "PR",
        }
    }

    /// The paper that defines this kind: (bibkey, DOI).
    const fn paper(self) -> (&'static str, Option<&'static str>) {
        match self {
            Kind::Vdw => ("vanderWaals-1873", None),
            Kind::Srk => ("Soave-CES-1972", Some("10.1016/0009-2509(72)80096-4")),
            Kind::Pr => ("PengRobinson-IECF-1976", Some("10.1021/i160057a011")),
        }
    }
}

/// The cubic `kind` of `source`, a pure fluid with a Helmholtz model: T_c and p_c from its published critical point
/// and ω from its acentric factor, the canonical record (ROT-216); α `alpha`, or else Soave's from ω (van der Waals:
/// α = 1); `data` chooses the Ω constants (DIV-0047). The fluid is named `PR::<name>` (see [`Kind::prefix`]) with the
/// source's aliases prefixed, takes its molar mass, limits and acentric factor, and its critical point is the cubic's
/// own (T_c, p_c, ρ_c = p_c/(Z_c R T_c); `CriticalOrigin::Model`, ROT-217); SRK and PR fluids take Bell and Deiters'
/// superancillary as their saturation curve ([`CubicSaturation`]). A source without a Helmholtz model, a
/// published critical point or an acentric factor is `NoModel`.
pub fn fluid(source: &Fluid, kind: Kind, alpha: Option<Alpha>, data: DataSet) -> Result<Fluid, Error> {
    Ok(Fluid::new(Arc::new(pure(source, kind, alpha, data)?)))
}

/// [`fluid`]'s package.
fn pure(source: &Fluid, kind: Kind, alpha: Option<Alpha>, data: DataSet) -> Result<PureFluid, Error> {
    let missing = |c: Constant| Error::NoModel { prop: Prop::Constant(c) };
    let info = source.info();
    let critical = info.published_critical().ok_or(missing(Constant::TCritical))?;
    let omega = info.constants().acentric.ok_or(missing(Constant::Acentric))?;
    let limits = source.model().limits().ok_or(missing(Constant::TMax))?;
    if source.model().helmholtz().is_none() {
        return Err(missing(Constant::GasConstant));
    }
    let alpha = alpha.unwrap_or(match kind.soave_m(omega) {
        Some(m) => Alpha::Soave(m),
        None => Alpha::One,
    });
    let (t_c, p_c) = (critical.t, critical.p);
    let rho_c = p_c / (kind.z_c() * R_CODATA * t_c);
    let name = format!("{}::{}", kind.prefix(), info.name());
    let aliases: Vec<String> = info.aliases().map(|alias| format!("{}::{alias}", kind.prefix())).collect();
    let aliases: Vec<&str> = aliases.iter().map(String::as_str).collect();
    let (bibkey, doi) = kind.paper();
    let key = format!("{name}|{alpha:?}|{data:?}|{t_c:e}|{p_c:e}|{omega:e}|{:?}", info.key());
    let reducing = Reducing { t: t_c, rho: rho_c, p: Some(p_c) };
    let critical = CriticalPoint { t: t_c, p: p_c, rho: rho_c, origin: CriticalOrigin::Model };
    let info = FluidInfo::new(
        &name,
        info.molar_mass(),
        Source::new(bibkey, doi, DataTerms::Published),
        ModelKey::from_content(key.as_bytes()),
    )?
    .with_aliases(&aliases)
    .with_published_critical(CriticalPoint { origin: CriticalOrigin::Published, ..critical })
    .with_constants(Constants::new(Some(reducing), Some(omega), None));
    let residual = Residual::new(kind, data, alpha, t_c, p_c);
    let model = CubicModel { residual, source: source.clone() };
    let builder = PureFluid::builder(info, model, *limits).critical(critical);
    Ok(match CubicSaturation::new(residual) {
        Some(curve) => builder.saturation(curve).build(),
        None => builder.build(),
    })
}

/// `registry` with the cubic `kinds` of each of its pure fluids that [`fluid`] can build, under `data`'s constants.
/// Pseudo-pure blends (their saturation is `SatAccuracy::Definition`) and fluids without a published critical point or
/// acentric factor are left out.
pub fn with_cubics(registry: &Registry, kinds: &[Kind], data: DataSet) -> Result<Registry, Error> {
    let mut out = registry.clone();
    for name in registry.names() {
        let source = registry.get(name)?;
        let package = (source.model() as &dyn Any).downcast_ref::<PureFluid>();
        let blend =
            package.and_then(|p| p.saturation().ok().flatten()).map(|c| c.accuracy()) == Some(SatAccuracy::Definition);
        if package.is_none() || blend {
            continue;
        }
        for &kind in kinds {
            match pure(source, kind, None, data) {
                Ok(cubic) => out = out.with_model(Arc::new(cubic))?,
                Err(Error::NoModel { .. }) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(out)
}
