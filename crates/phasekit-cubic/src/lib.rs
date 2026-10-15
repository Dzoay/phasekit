//! `phasekit-cubic`: the van der Waals, Soave-Redlich-Kwong and Peng-Robinson cubic equations of state as a model
//! family on phasekit-core's public seams (D3; PLAN.md M11). A cubic fluid takes its critical point and acentric
//! factor from a canonical HEOS fluid and borrows that fluid's ideal gas (ARCHITECTURE.md §11; map 06 U1-U5).
//!
//! Its milestone holds phasekit-core unchanged (`cargo xtask gates core-frozen`): everything here is built on the
//! public API.

/// The molar gas constant of every cubic model, J/(mol·K): CODATA 2018's, exact since the 2019 SI redefinition, and
/// CoolProp 8.0.0's `R_U_CODATA` (user decision CB1). It scales a cubic's α^r and the ideal gas it borrows alike, in
/// both datasets, so a cubic fluid's c_p⁰ is its source's times `R_CODATA`/R_source (DIV-0046).
pub const R_CODATA: f64 = 8.314_462_618_153_24;

#[cfg(test)]
mod tests {
    use super::*;

    /// User decision CB1: R is the product of the SI's exact Avogadro and Boltzmann constants, to one ulp, and prints as
    /// CODATA 2018's 8.31446261815324.
    #[test]
    fn r_is_avogadro_times_boltzmann() {
        let (n_a, k_b) = (6.022_140_76e23, 1.380_649e-23);
        assert!((n_a * k_b - R_CODATA).abs() <= f64::EPSILON * R_CODATA, "{}", n_a * k_b);
        assert_eq!(R_CODATA.to_string(), "8.31446261815324");
    }
}
