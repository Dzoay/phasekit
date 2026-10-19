//! M22's bench mixtures (PLAN.md M22.1), which its batch benches and tests share: R32/R125 and Methane/Ethane at 50/50
//! and a five-component natural gas on CoolProp 8.0.0's library, and AGA8's 21-component check gas on GERG-2008. The
//! kit reads no fluid, so each mixture is its components and mole fractions, built by `phasekit-mix` where it is used.

/// The multi-fluid model a bench mixture is built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixtureModel {
    /// CoolProp 8.0.0's library over the registry's fluids (`phasekit_mix::mixture`).
    Library,
    /// GERG-2008 over its own 21 components (`phasekit_mix::gerg2008`).
    Gerg2008,
}

/// A bench mixture: its label in bench ids, its model, and its components by registry name with their mole fractions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BenchMixture {
    /// The label: the fluid of a bench row.
    pub label: &'static str,
    /// The model it is built with.
    pub model: MixtureModel,
    /// The components and their mole fractions, summing to 1.
    pub components: &'static [(&'static str, f64)],
}

/// AGA8's check composition (`AGA8CODE/C/GERG2008_test_01.cpp`), GERG-2008's 21 components in AGA8's order.
const AGA8_GAS: [(&str, f64); 21] = [
    ("Methane", 0.77824),
    ("Nitrogen", 0.02),
    ("CarbonDioxide", 0.06),
    ("Ethane", 0.08),
    ("n-Propane", 0.03),
    ("IsoButane", 0.0015),
    ("n-Butane", 0.003),
    ("Isopentane", 0.0005),
    ("n-Pentane", 0.00165),
    ("n-Hexane", 0.00215),
    ("n-Heptane", 0.00088),
    ("n-Octane", 0.00024),
    ("n-Nonane", 0.00015),
    ("n-Decane", 0.00009),
    ("Hydrogen", 0.004),
    ("Oxygen", 0.005),
    ("CarbonMonoxide", 0.002),
    ("Water", 0.0001),
    ("HydrogenSulfide", 0.0025),
    ("Helium", 0.007),
    ("Argon", 0.001),
];

/// The bench mixtures (PLAN.md M22.1). `NaturalGas5` is the library's model of `mixture_alphar`'s GERG-2008 gas.
pub const BENCH_MIXTURES: [BenchMixture; 4] = [
    BenchMixture { label: "R32&R125", model: MixtureModel::Library, components: &[("R32", 0.5), ("R125", 0.5)] },
    BenchMixture {
        label: "Methane&Ethane",
        model: MixtureModel::Library,
        components: &[("Methane", 0.5), ("Ethane", 0.5)],
    },
    BenchMixture {
        label: "NaturalGas5",
        model: MixtureModel::Library,
        components: &[
            ("Methane", 0.85),
            ("Ethane", 0.07),
            ("n-Propane", 0.03),
            ("Nitrogen", 0.03),
            ("CarbonDioxide", 0.02),
        ],
    },
    BenchMixture { label: "AGA8Gas21", model: MixtureModel::Gerg2008, components: &AGA8_GAS },
];
