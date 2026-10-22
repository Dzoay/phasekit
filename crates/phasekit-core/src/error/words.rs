//! The words a long-form message names a property, a constant or a strategy by, beside its key (user decision EM7): `the
//! isobaric heat capacity (Cpmass)`. The key carries the basis, so a molar and a mass property share their words. The
//! short form keeps the key alone.

use crate::prop::{Constant, Prop};
use crate::state::Strategy;

/// A property's words, or `None` for a derivative, which its notation names.
pub(super) const fn prop(prop: Prop) -> Option<&'static str> {
    Some(match prop {
        Prop::T => "temperature",
        Prop::P => "pressure",
        Prop::Q => "quality",
        Prop::Qmass => "mass quality",
        Prop::Dmolar | Prop::Dmass => "density",
        Prop::Hmolar | Prop::Hmass => "enthalpy",
        Prop::Smolar | Prop::Smass => "entropy",
        Prop::Umolar | Prop::Umass => "internal energy",
        Prop::Gmolar | Prop::Gmass => "Gibbs energy",
        Prop::Helmholtzmolar | Prop::Helmholtzmass => "Helmholtz energy",
        Prop::Cvmolar | Prop::Cvmass => "isochoric heat capacity",
        Prop::Cpmolar | Prop::Cpmass => "isobaric heat capacity",
        Prop::Cp0molar | Prop::Cp0mass => "ideal-gas isobaric heat capacity",
        Prop::SpeedOfSound => "speed of sound",
        Prop::Z => "compressibility factor",
        Prop::HmolarResidual => "residual enthalpy",
        Prop::SmolarResidual => "residual entropy",
        Prop::GmolarResidual => "residual Gibbs energy",
        Prop::Bvirial => "second virial coefficient",
        Prop::Cvirial => "third virial coefficient",
        Prop::DBvirialDT => "second virial coefficient's temperature derivative",
        Prop::DCvirialDT => "third virial coefficient's temperature derivative",
        Prop::Tau => "inverse reduced temperature",
        Prop::Delta => "reduced density",
        Prop::HmolarIdealGas | Prop::HmassIdealGas => "ideal-gas enthalpy",
        Prop::SmolarIdealGas | Prop::SmassIdealGas => "ideal-gas entropy",
        Prop::UmolarIdealGas | Prop::UmassIdealGas => "ideal-gas internal energy",
        Prop::Viscosity => "viscosity",
        Prop::Conductivity => "thermal conductivity",
        Prop::SurfaceTension => "surface tension",
        Prop::MeltingPressure => "melting pressure",
        Prop::MeltingTemperature => "melting temperature",
        Prop::FundamentalDerivative => "fundamental derivative of gas dynamics",
        Prop::IsothermalCompressibility => "isothermal compressibility",
        Prop::IsobaricExpansion => "isobaric expansion coefficient",
        Prop::IsentropicExpansion => "isentropic expansion coefficient",
        Prop::Prandtl => "Prandtl number",
        Prop::Pip => "phase identification parameter",
        Prop::Alphar => "reduced residual Helmholtz energy",
        Prop::DalpharDtau => "reduced residual Helmholtz energy's τ-derivative",
        Prop::DalpharDdelta => "reduced residual Helmholtz energy's δ-derivative",
        Prop::Alpha0 => "reduced ideal-gas Helmholtz energy",
        Prop::Dalpha0Dtau => "reduced ideal-gas Helmholtz energy's τ-derivative",
        Prop::Dalpha0Ddelta => "reduced ideal-gas Helmholtz energy's δ-derivative",
        Prop::D2alpha0Ddelta2 => "reduced ideal-gas Helmholtz energy's second δ-derivative",
        Prop::D3alpha0Ddelta3 => "reduced ideal-gas Helmholtz energy's third δ-derivative",
        Prop::Constant(c) => constant(c),
        Prop::Partial(_) | Prop::SecondPartial(_) => return None,
    })
}

pub(super) const fn constant(constant: Constant) -> &'static str {
    match constant {
        Constant::GasConstant => "molar gas constant",
        Constant::MolarMass => "molar mass",
        Constant::Acentric => "acentric factor",
        Constant::DipoleMoment => "dipole moment",
        Constant::RhomassReducing | Constant::RhomolarReducing => "reducing density",
        Constant::RhomassCritical | Constant::RhomolarCritical => "critical density",
        Constant::TReducing => "reducing temperature",
        Constant::TCritical => "critical temperature",
        Constant::TTriple => "triple-point temperature",
        Constant::TMax => "maximum temperature",
        Constant::TMin => "minimum temperature",
        Constant::PMin => "minimum pressure",
        Constant::PMax => "maximum pressure",
        Constant::PCritical => "critical pressure",
        Constant::PReducing => "reducing pressure",
        Constant::PTriple => "triple-point pressure",
        Constant::FractionMin => "minimum fraction",
        Constant::FractionMax => "maximum fraction",
        Constant::TFreeze => "freezing temperature",
        Constant::Gwp20 => "20-year global warming potential",
        Constant::Gwp100 => "100-year global warming potential",
        Constant::Gwp500 => "500-year global warming potential",
        Constant::Fh => "flammability hazard",
        Constant::Hh => "health hazard",
        Constant::Ph => "physical hazard",
        Constant::Odp => "ozone depletion potential",
    }
}

pub(super) const fn strategy(strategy: Strategy) -> &'static str {
    match strategy {
        Strategy::Direct => "direct evaluation",
        Strategy::External => "model's own flash",
        Strategy::DensityNewton => "density Newton iteration",
        Strategy::Superancillary => "superancillary evaluation",
        Strategy::Vle => "vapour-liquid equilibrium solve",
        Strategy::Ancillary => "ancillary saturation",
        Strategy::Isobar => "search along the isobar",
        Strategy::TRhoNewton => "temperature-density Newton iteration",
        Strategy::Isochore => "search along the isochore",
        Strategy::Isotherm => "search along the isotherm",
        Strategy::Saturation => "search along the saturation curve",
        Strategy::Isentrope => "search along the isentrope",
        Strategy::Melting => "search along the melting curve",
        Strategy::Conformal => "conformal-state Newton iteration",
    }
}
