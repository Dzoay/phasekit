//! The 318 active transport rows of CoolProp v8.0.0's tests (`CoolProp-Tests.cpp:44-301, 347-581`), one file per fluid
//! and property, each with its independent second transcription (PLAN.md M8.3; VERIFICATION.md §4.2), and the phase a
//! row's state takes inside the dome.

#![allow(dead_code, reason = "each test binary that includes the rows uses its part of them")]

use phasekit_core::{Basis, Fluid, Phase, State};
use phasekit_verify::fixture;

/// The single phase a check value at a (T, ρ) inside the dome is evaluated at, on its side of ρ_c, metastable, as its
/// paper evaluates the correlation (M8.10: transport refuses a two-phase state, ROT-010); `None` for any other state,
/// a saturated phase among them.
pub fn dome_side(fluid: &Fluid, state: &State) -> Option<Phase> {
    state.quality().filter(|q| 0.0 < *q && *q < 1.0)?;
    let rho_c = fluid.model().critical_point()?.rho;
    Some(if state.rho(Basis::Molar) < rho_c { Phase::Gas } else { Phase::Liquid })
}

/// A committed fixture: (path, text).
pub type Text = (&'static str, &'static str);

/// Every file and its `.check.csv`.
pub const ROWS: &[(Text, Text)] = &[
    (fixture!("paper/Air/CoolProp-Tests.eta.csv"), fixture!("paper/Air/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Air/CoolProp-Tests.lambda.csv"), fixture!("paper/Air/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Ammonia/CoolProp-Tests.eta.csv"), fixture!("paper/Ammonia/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Ammonia/CoolProp-Tests.lambda.csv"), fixture!("paper/Ammonia/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Argon/CoolProp-Tests.eta.csv"), fixture!("paper/Argon/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Argon/CoolProp-Tests.lambda.csv"), fixture!("paper/Argon/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Benzene/CoolProp-Tests.eta.csv"), fixture!("paper/Benzene/CoolProp-Tests.eta.check.csv")),
    (
        fixture!("paper/CarbonDioxide/CoolProp-Tests.eta.csv"),
        fixture!("paper/CarbonDioxide/CoolProp-Tests.eta.check.csv"),
    ),
    (
        fixture!("paper/CarbonDioxide/CoolProp-Tests.lambda.csv"),
        fixture!("paper/CarbonDioxide/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/CycloHexane/CoolProp-Tests.eta.csv"), fixture!("paper/CycloHexane/CoolProp-Tests.eta.check.csv")),
    (
        fixture!("paper/Cyclopentane/CoolProp-Tests.lambda.csv"),
        fixture!("paper/Cyclopentane/CoolProp-Tests.lambda.check.csv"),
    ),
    (
        fixture!("paper/DimethylEther/CoolProp-Tests.eta.csv"),
        fixture!("paper/DimethylEther/CoolProp-Tests.eta.check.csv"),
    ),
    (fixture!("paper/Ethane/CoolProp-Tests.eta.csv"), fixture!("paper/Ethane/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Ethane/CoolProp-Tests.lambda.csv"), fixture!("paper/Ethane/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Ethanol/CoolProp-Tests.eta.csv"), fixture!("paper/Ethanol/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Ethanol/CoolProp-Tests.lambda.csv"), fixture!("paper/Ethanol/CoolProp-Tests.lambda.check.csv")),
    (
        fixture!("paper/EthylBenzene/CoolProp-Tests.eta.csv"),
        fixture!("paper/EthylBenzene/CoolProp-Tests.eta.check.csv"),
    ),
    (
        fixture!("paper/EthylBenzene/CoolProp-Tests.lambda.csv"),
        fixture!("paper/EthylBenzene/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/HeavyWater/CoolProp-Tests.eta.csv"), fixture!("paper/HeavyWater/CoolProp-Tests.eta.check.csv")),
    (
        fixture!("paper/HeavyWater/CoolProp-Tests.lambda.csv"),
        fixture!("paper/HeavyWater/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/Helium/CoolProp-Tests.eta.csv"), fixture!("paper/Helium/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Helium/CoolProp-Tests.lambda.csv"), fixture!("paper/Helium/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Hydrogen/CoolProp-Tests.eta.csv"), fixture!("paper/Hydrogen/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Hydrogen/CoolProp-Tests.lambda.csv"), fixture!("paper/Hydrogen/CoolProp-Tests.lambda.check.csv")),
    (
        fixture!("paper/HydrogenSulfide/CoolProp-Tests.eta.csv"),
        fixture!("paper/HydrogenSulfide/CoolProp-Tests.eta.check.csv"),
    ),
    (fixture!("paper/IsoButane/CoolProp-Tests.eta.csv"), fixture!("paper/IsoButane/CoolProp-Tests.eta.check.csv")),
    (
        fixture!("paper/IsoButane/CoolProp-Tests.lambda.csv"),
        fixture!("paper/IsoButane/CoolProp-Tests.lambda.check.csv"),
    ),
    (
        fixture!("paper/Isopentane/CoolProp-Tests.lambda.csv"),
        fixture!("paper/Isopentane/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/Methane/CoolProp-Tests.lambda.csv"), fixture!("paper/Methane/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Methanol/CoolProp-Tests.eta.csv"), fixture!("paper/Methanol/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Methanol/CoolProp-Tests.lambda.csv"), fixture!("paper/Methanol/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Nitrogen/CoolProp-Tests.eta.csv"), fixture!("paper/Nitrogen/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Nitrogen/CoolProp-Tests.lambda.csv"), fixture!("paper/Nitrogen/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/Oxygen/CoolProp-Tests.eta.csv"), fixture!("paper/Oxygen/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Oxygen/CoolProp-Tests.lambda.csv"), fixture!("paper/Oxygen/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/R123/CoolProp-Tests.eta.csv"), fixture!("paper/R123/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/R123/CoolProp-Tests.lambda.csv"), fixture!("paper/R123/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/R125/CoolProp-Tests.eta.csv"), fixture!("paper/R125/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/R125/CoolProp-Tests.lambda.csv"), fixture!("paper/R125/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/R134a/CoolProp-Tests.eta.csv"), fixture!("paper/R134a/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/R134a/CoolProp-Tests.lambda.csv"), fixture!("paper/R134a/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/R23/CoolProp-Tests.eta.csv"), fixture!("paper/R23/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/R23/CoolProp-Tests.lambda.csv"), fixture!("paper/R23/CoolProp-Tests.lambda.check.csv")),
    (
        fixture!("paper/SulfurHexafluoride/CoolProp-Tests.eta.csv"),
        fixture!("paper/SulfurHexafluoride/CoolProp-Tests.eta.check.csv"),
    ),
    (
        fixture!("paper/SulfurHexafluoride/CoolProp-Tests.lambda.csv"),
        fixture!("paper/SulfurHexafluoride/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/Toluene/CoolProp-Tests.eta.csv"), fixture!("paper/Toluene/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Water/CoolProp-Tests.eta.csv"), fixture!("paper/Water/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/Water/CoolProp-Tests.lambda.csv"), fixture!("paper/Water/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/m-Xylene/CoolProp-Tests.eta.csv"), fixture!("paper/m-Xylene/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/m-Xylene/CoolProp-Tests.lambda.csv"), fixture!("paper/m-Xylene/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/n-Butane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Butane/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/n-Butane/CoolProp-Tests.lambda.csv"), fixture!("paper/n-Butane/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/n-Decane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Decane/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/n-Dodecane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Dodecane/CoolProp-Tests.eta.check.csv")),
    (
        fixture!("paper/n-Dodecane/CoolProp-Tests.lambda.csv"),
        fixture!("paper/n-Dodecane/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/n-Heptane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Heptane/CoolProp-Tests.eta.check.csv")),
    (
        fixture!("paper/n-Heptane/CoolProp-Tests.lambda.csv"),
        fixture!("paper/n-Heptane/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/n-Hexane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Hexane/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/n-Hexane/CoolProp-Tests.lambda.csv"), fixture!("paper/n-Hexane/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/n-Nonane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Nonane/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/n-Nonane/CoolProp-Tests.lambda.csv"), fixture!("paper/n-Nonane/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/n-Octane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Octane/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/n-Octane/CoolProp-Tests.lambda.csv"), fixture!("paper/n-Octane/CoolProp-Tests.lambda.check.csv")),
    (
        fixture!("paper/n-Pentane/CoolProp-Tests.lambda.csv"),
        fixture!("paper/n-Pentane/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/n-Propane/CoolProp-Tests.eta.csv"), fixture!("paper/n-Propane/CoolProp-Tests.eta.check.csv")),
    (
        fixture!("paper/n-Propane/CoolProp-Tests.lambda.csv"),
        fixture!("paper/n-Propane/CoolProp-Tests.lambda.check.csv"),
    ),
    (fixture!("paper/o-Xylene/CoolProp-Tests.eta.csv"), fixture!("paper/o-Xylene/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/o-Xylene/CoolProp-Tests.lambda.csv"), fixture!("paper/o-Xylene/CoolProp-Tests.lambda.check.csv")),
    (fixture!("paper/p-Xylene/CoolProp-Tests.eta.csv"), fixture!("paper/p-Xylene/CoolProp-Tests.eta.check.csv")),
    (fixture!("paper/p-Xylene/CoolProp-Tests.lambda.csv"), fixture!("paper/p-Xylene/CoolProp-Tests.lambda.check.csv")),
];
