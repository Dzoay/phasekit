//! What `DataSet::Corrected` changes in v8.0.0's mixture library, each by its divergence entry (VERIFICATION.md §6.4).
//! `Parity` keeps the library's values. A correction names its pair by CAS (sorted, as the pairs are keyed) and the
//! v8.0.0 value it replaces, which a test checks bit for bit against the generated table.

use crate::data::Reducing;

/// One corrected reducing parameter of a pair: its index in GERG's [β_T, γ_T, β_v, γ_v], oriented to the sorted CAS
/// pair as the library's are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Correction {
    pub cas: [&'static str; 2],
    pub parameter: usize,
    pub v800: f64,
    pub corrected: f64,
    pub divergence: &'static str,
}

/// DIV-0013: R1234yf/R1234ze(E)'s γ_T is 0.993309 in Bell et al. 2022 Table III and in NIST IR 8570's supplement
/// (`HMX_DOE.bnc`); v8.0.0's library has 0.999309, which moves T_r by 1.08 K and Table XI's α^r by 0.89 %.
pub(crate) const CORRECTIONS: [Correction; 1] = [Correction {
    cas: ["29118-24-9", "754-12-1"],
    parameter: 1,
    v800: 0.999309,
    corrected: 0.993309,
    divergence: "DIV-0013",
}];

/// `parameters` of the pair keyed `cas`, corrected.
pub(crate) fn corrected(cas: [&str; 2], parameters: Reducing) -> Reducing {
    let Reducing::Gerg(mut p) = parameters else { return parameters };
    for c in CORRECTIONS.iter().filter(|c| c.cas == cas) {
        if let Some(value) = p.get_mut(c.parameter) {
            *value = c.corrected;
        }
    }
    Reducing::Gerg(p)
}
