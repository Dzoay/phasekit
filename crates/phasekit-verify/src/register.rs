//! The compiled divergence register (VERIFICATION.md §6): every deliberate difference from the CoolProp 8.0.0
//! oracle is a typed, cited entry, cross-checked against the data corrections and proved by tests that run once their
//! milestone is reached (`tests/divergences.rs`). Literature arbitration as executable TDD.

use crate::fixture::Kind;
use crate::tolerance::Tolerance;
use phasekit_core::internal::Patch;

/// What part of a model the divergence concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum Part {
    GasConstant,
    Reducing,
    Melting,
    Transport,
    Algorithm,
}

/// What the tests do about it (VERIFICATION.md §6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Policy {
    /// The arbiter wins; a data patch or algorithm fix implements it. Requires an arbiter.
    UsePaper,
    /// The oracle is wrong by construction (e.g. two-phase transport); oracle rows are not asserted.
    SkipOracle,
    /// Accepted, documented divergence: the oracle value is kept on purpose, so no patch may cite the entry
    /// (Corrected = Parity there), and the literature rows are asserted at the measured tolerance.
    KeepOracle,
    /// Cause unresolved; neither side arbitrates yet; the measured residual is the tolerance.
    Investigate,
}

/// Lifecycle of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DivStatus {
    /// The oracle still differs (proof part ii holds).
    Open,
    /// A newer oracle agrees with the arbiter; kept for the audit trail.
    ResolvedUpstream {
        /// CoolProp commit that fixed it.
        commit: &'static str,
    },
}

/// How the divergence is implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fix {
    /// A `data/corrections.csv` patch on the Corrected dataset; every patch cites its entry.
    Data,
    /// A code path, named by its module (`phasekit_core::state`), that must exist once the proof is due.
    Code(&'static str),
    /// Nothing changes: `KeepOracle` and `Investigate`.
    None,
}

/// Which rows of an exempt kind are not asserted against the oracle.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Rows {
    /// Every row.
    All,
    /// Two-phase rows (0 < Q < 1).
    TwoPhase,
    /// Rows outside the model's domain (the `.edge` files).
    BelowDomain,
    /// Rows with lo ≤ T ≤ hi.
    TBand {
        /// Lower temperature (K).
        lo: f64,
        /// Upper temperature (K).
        hi: f64,
    },
    /// Rows given p (the `sat` kind's PQ rows, input `p`).
    GivenP,
}

/// The oracle cells a `SkipOracle` entry does not assert on Parity; the tests count them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exempt {
    /// Fixture kinds.
    pub kinds: &'static [Kind],
    /// Columns (`props` outputs for the long format); `"*"` for every column.
    pub columns: &'static [&'static str],
    /// Rows.
    pub rows: Rows,
}

/// One register entry (VERIFICATION.md §6.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divergence {
    /// "DIV-0001"; never reused or renumbered.
    pub id: &'static str,
    /// Canonical names, "A&B" for a mixture, `["*"]` for every fluid.
    pub fluids: &'static [&'static str],
    /// GasConstant | Reducing | Melting | Transport | Algorithm.
    pub part: Part,
    /// "<bibkey> <table>"; required for UsePaper.
    pub arbiter: Option<&'static str>,
    /// UsePaper | SkipOracle | KeepOracle | Investigate.
    pub policy: Policy,
    /// Data (a corrections.csv patch) | Code(module) | None.
    pub fix: Fix,
    /// Map section, measurement (value, date), upstream commit.
    pub evidence: &'static str,
    /// SkipOracle only: oracle cells not asserted on Parity.
    pub exempt: Option<Exempt>,
    /// The Measured bound: literature rows (KeepOracle, Investigate) or oracle columns that stay asserted beside an
    /// exemption.
    pub tolerance: Option<Tolerance>,
    /// Milestones whose PRs add the proof's parts, e.g. `&[5, 6]`.
    pub proof: &'static [u8],
    /// Open | ResolvedUpstream { commit }.
    pub status: DivStatus,
}

/// The register, seeded from map 12 §6.3 and the map 10 §8.5 seed list (E15) with VERIFICATION.md §6.6's values; it
/// grows by PR with a proof per entry. `Investigate` stubs become `UsePaper` once their arbiter row is transcribed
/// (M1-M8), or `KeepOracle` when the user accepts the oracle value (DIV-0005, docs/design/04-user-decisions.md).
pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "DIV-0001",
        fluids: &["R1234ze(E)"],
        part: Part::GasConstant,
        arbiter: Some("Thol-IJT-2016-R1234zeE Table 3"),
        policy: Policy::UsePaper,
        fix: Fix::Data,
        evidence: "map 13 §3 item 3: stored R 8.314472, paper R 8.3144621",
        exempt: None,
        tolerance: None,
        proof: &[5, 6],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0002",
        fluids: &["Water"],
        part: Part::Melting,
        arbiter: Some("IAPWS R14-08"),
        policy: Policy::UsePaper,
        fix: Fix::Data,
        evidence: "map 10 R10: ice VI p0 623.4 MPa vs 632.4 MPa",
        exempt: None,
        tolerance: None,
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0003",
        fluids: &["Nitrogen"],
        part: Part::Reducing,
        arbiter: Some("Span-JPCRD-2000"),
        policy: Policy::UsePaper,
        fix: Fix::Data,
        evidence: "map 12 §6.3: rho_r 11183.901464580624 vs 11183.9 (upstream 2acbbc82)",
        exempt: None,
        tolerance: None,
        proof: &[2, 6],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0004",
        fluids: &["*"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::Code("phasekit_core::state"),
        evidence: "map 05 §6, map 10 R18: two-phase viscosity/conductivity are meaningless",
        exempt: Some(Exempt {
            kinds: &[Kind::Props],
            columns: &["Cpmass", "Cpmolar", "Cvmass", "Cvmolar", "viscosity", "conductivity"],
            rows: Rows::TwoPhase,
        }),
        tolerance: None,
        proof: &[5, 8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0005",
        fluids: &["Helium"],
        part: Part::GasConstant,
        arbiter: None,
        policy: Policy::KeepOracle,
        fix: Fix::None,
        evidence: "map 13 §3.4, R3: R 8.3144598 kept; IR 8474 Table 3 fails with its Table 1 R; tolerance 5e-7",
        exempt: None,
        tolerance: Some(Tolerance::Relative(5e-7)),
        proof: &[5, 6],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0006",
        fluids: &["Ethylene"],
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 12 §6.3: v8.0.0 rho_r 7636.76598074554, M 0.02805376 (fixed 2acbbc82); row to transcribe",
        exempt: None,
        tolerance: None,
        proof: &[2],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0007",
        fluids: &["OrthoHydrogen"],
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 12 §6.3: v8.0.0 rho_r 15444.54031369981, M 0.00201594 (fixed 2acbbc82); row to transcribe",
        exempt: None,
        tolerance: None,
        proof: &[2],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0008",
        fluids: &["n-Undecane"],
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 12 §6.3: v8.0.0 rho_r 1514.916863638556 (fixed 2acbbc82); row to transcribe",
        exempt: None,
        tolerance: None,
        proof: &[2],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0009",
        fluids: &["R1233zd(E)"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 12 §6.3: v8.0.0 raises 'Viscosity model is not available'; restored upstream 14da1f0d",
        exempt: None,
        tolerance: None,
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0010",
        fluids: &["*"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::Code("phasekit_cubic"),
        evidence: "map 12 §6.3: PR/SRK entropy inconsistent (T(ds/dT)p 91.35 vs cp 93.89); identity arbitrates",
        exempt: None,
        tolerance: None,
        proof: &[11],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0011",
        fluids: &["*"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::Code("phasekit_core::helmholtz"),
        evidence: "map 12 §6.3, R8: Cvirial from delta = 1e-12 off -6.7e-5/-7.1e-5/+1.9e-5; exact Taylor path (E4); \
                   M5.6: B, dB/dT off up to 1.5e-8, 8.6e-8 (Methanol), the series confirmed from its JSON",
        exempt: Some(Exempt { kinds: &[Kind::Eos], columns: &["Cvirial", "dCvirial_dT"], rows: Rows::All }),
        tolerance: Some(Tolerance::Relative(9e-8)),
        proof: &[5],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0012",
        fluids: &["Water"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::Code("phasekit_core::flash"),
        evidence: "map 12 §6.3: DT at 250 K < Tmin gives p = -5.928 Pa without error; we refuse (DomainError)",
        exempt: Some(Exempt { kinds: &[Kind::Eos, Kind::Flash], columns: &["*"], rows: Rows::BelowDomain }),
        tolerance: None,
        proof: &[5],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0013",
        fluids: &["R1234yf&R1234ze(E)"],
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.4: mixture alphar -0.464679 vs Bell 2022 Table XI -0.460595; reducing T differs (M13)",
        exempt: None,
        tolerance: None,
        proof: &[13],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0014",
        fluids: &["R1224YDZ"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.4: p(400 K, 8000 mol/m3) 21.1790735 MPa vs printed 21.17909 (3.3x half a digit)",
        exempt: None,
        tolerance: Some(Tolerance::Relative(8e-7)),
        proof: &[5],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0015",
        fluids: &["R123"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 02 §6, map 13 A4: c_p0 blocks Tc 456.82 vs T_r 456.831; c_p0 -1.33e-5 at 300 K (M4.3)",
        exempt: None,
        tolerance: None,
        proof: &[4],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0016",
        fluids: &["PropyleneGlycol"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::None,
        evidence: "map 10 §8.1, M6.4: fastchebpure 2026.06.02-v2 below 227.6028 K (its first interval): rho'' 0.7 % below the v8 EOS's \
                   saturation at 213 K, 1.1e-11 at 225.46 K; CoolProp 8.0.0's VLE agrees with phasekit's to 6e-15",
        exempt: Some(Exempt {
            kinds: &[Kind::Checkpoints],
            columns: &["p", "rhoV"],
            rows: Rows::TBand { lo: 213.0, hi: 227.6028 },
        }),
        tolerance: None,
        proof: &[6],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0017",
        fluids: &["Chlorine", "DimethylCarbonate"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::None,
        evidence: "map 02 §6, M6.7: the oracle's computed critical point is less converged than the EOS's own on flat \
                   critical isotherms: DimethylCarbonate rhoc_num 2.0e-6 off (K2 7.5e-11 there, dK2/dln rho 3.7e-5); \
                   Chlorine's critical region is degenerate (K1, K2 within 1e-12 of 0 from 7950 to 8153 mol/m3)",
        exempt: Some(Exempt { kinds: &[Kind::Crit], columns: &["Tc_num", "pc_num", "rhoc_num"], rows: Rows::All }),
        tolerance: None,
        proof: &[6],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0018",
        fluids: &["*"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::None,
        evidence: "map 03 §3.3, M6.8: CoolProp's PQ takes T from its T(ln p) inverse with no polish (FlashRoutines.cpp:1169), \
                   so its PQ states miss its own superancillary: up to 2.0e-11 in T and 1.1e-6 in rho at Theta ~ 1e-7, \
                   4.5e-14 in T at Theta ~ 0.1; phasekit's PQ solves the curve's p(T) = p to rounding",
        exempt: Some(Exempt {
            kinds: &[Kind::Sat],
            columns: &["T", "rhoL", "rhoV", "hL", "hV", "sL", "sV"],
            rows: Rows::GivenP,
        }),
        tolerance: None,
        proof: &[6],
        status: DivStatus::Open,
    },
];

/// The register entry, if any, whose `exempt` cells include `column` of a `kind` row of `fluid` at temperature `t`: an
/// exemption of every row or of a temperature band (the others need more than T to decide).
pub fn exempt_at(register: &[Divergence], fluid: &str, kind: Kind, column: &str, t: f64) -> Option<&'static str> {
    exempt_row(register, fluid, kind, column, t, None)
}

/// [`exempt_at`] for a row whose input label (the `sat` kind's `T`, `p` or `sa`) is known too: [`Rows::GivenP`]
/// exempts the rows given p.
pub fn exempt_row(
    register: &[Divergence],
    fluid: &str,
    kind: Kind,
    column: &str,
    t: f64,
    input: Option<&str>,
) -> Option<&'static str> {
    let applies = |d: &&Divergence| {
        let Some(e) = &d.exempt else { return false };
        let rows = match e.rows {
            Rows::All => true,
            Rows::TBand { lo, hi } => lo <= t && t <= hi,
            Rows::GivenP => input == Some("p"),
            _ => false,
        };
        (d.fluids.contains(&fluid) || d.fluids == ["*"])
            && e.kinds.contains(&kind)
            && e.columns.contains(&column)
            && rows
    };
    register.iter().find(applies).map(|d| d.id)
}

/// Why the register and the corrections disagree (VERIFICATION.md §7.2).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegisterError {
    /// A patch cites an id that is not in the register.
    UnknownId(Box<str>),
    /// A patch cites an entry whose policy is not `UsePaper`, or that has no arbiter.
    NotUsePaper(Box<str>),
    /// Two entries share an id.
    DuplicateId(&'static str),
    /// A patch cites an entry whose `fix` is not `Data`.
    NotDataFix(Box<str>),
    /// A `UsePaper` entry with `fix: Data` that no patch implements.
    UncitedDataFix(&'static str),
    /// An entry whose evidence cites no map item ("map NN").
    NoMapCitation(&'static str),
}

/// Cross-checks the register against the patches shipped in the data (VERIFICATION.md §7.2).
pub fn check_register(register: &[Divergence], patches: &[Patch]) -> Result<(), RegisterError> {
    for (i, d) in register.iter().enumerate() {
        if register[..i].iter().any(|e| e.id == d.id) {
            return Err(RegisterError::DuplicateId(d.id));
        }
        if !cites_a_map(d.evidence) {
            return Err(RegisterError::NoMapCitation(d.id));
        }
    }
    for p in patches {
        let entry = register.iter().find(|d| *d.id == *p.divergence);
        match entry {
            None => return Err(RegisterError::UnknownId(p.divergence.clone())),
            Some(d) if d.policy != Policy::UsePaper || d.arbiter.is_none() => {
                return Err(RegisterError::NotUsePaper(p.divergence.clone()));
            }
            Some(d) if d.fix != Fix::Data => return Err(RegisterError::NotDataFix(p.divergence.clone())),
            Some(_) => {}
        }
    }
    let data_fixes = register.iter().filter(|d| d.policy == Policy::UsePaper && d.fix == Fix::Data);
    if let Some(d) = data_fixes.into_iter().find(|d| !patches.iter().any(|p| *p.divergence == *d.id)) {
        return Err(RegisterError::UncitedDataFix(d.id));
    }
    Ok(())
}

/// The ids of entries whose proof is due (one of its milestones is below `milestone`, the first open one) and that
/// have no proof among `proofs` (VERIFICATION.md §6.3).
pub fn missing_proofs(register: &[Divergence], milestone: u8, proofs: &[&str]) -> Vec<&'static str> {
    register
        .iter()
        .filter(|d| d.proof.iter().any(|m| *m < milestone) && !proofs.contains(&d.id))
        .map(|d| d.id)
        .collect()
}

/// The proof ids that name no register entry.
pub fn unregistered_proofs<'a>(register: &[Divergence], proofs: &[&'a str]) -> Vec<&'a str> {
    proofs.iter().copied().filter(|id| !register.iter().any(|d| d.id == *id)).collect()
}

/// Whether `evidence` cites a map item: "map " and two digits (`map 12 §6.3`, `map 05 R2`).
fn cites_a_map(evidence: &str) -> bool {
    evidence.match_indices("map ").any(|(i, _)| {
        let digits = evidence.as_bytes().get(i + 4..i + 6);
        digits.is_some_and(|d| d.iter().all(u8::is_ascii_digit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &'static str, proof: &'static [u8]) -> Divergence {
        Divergence { id, proof, ..DIVERGENCES[5] }
    }

    /// VERIFICATION.md §6.3: a proof is due once one of its milestones is below the first open milestone.
    #[test]
    fn proofs_fall_due_as_milestones_close() {
        let register = [entry("DIV-0101", &[5, 6]), entry("DIV-0102", &[2]), entry("DIV-0103", &[13])];
        assert_eq!(missing_proofs(&register, 2, &[]), Vec::<&str>::new(), "nothing is due while M2 is open");
        assert_eq!(missing_proofs(&register, 3, &[]), ["DIV-0102"]);
        assert_eq!(missing_proofs(&register, 6, &["DIV-0102"]), ["DIV-0101"], "due at its first milestone, M5");
        assert_eq!(missing_proofs(&register, 14, &["DIV-0101", "DIV-0102"]), ["DIV-0103"]);
        assert_eq!(unregistered_proofs(&register, &["DIV-0102", "DIV-0999"]), ["DIV-0999"]);
        assert_eq!(unregistered_proofs(&register, &[]), Vec::<&str>::new());
    }

    /// `exempt_at`: a band's ends are in and its outside is not, nor another column or fluid; `["*"]` covers every fluid
    /// and `Rows::All` every T; other row sets need more than T. DIV-0016 exempts PropyleneGlycol's check points, and
    /// DIV-0018 (through `exempt_row`) the PQ rows of the `sat` kind.
    #[test]
    fn exemptions_by_fluid_kind_column_and_temperature() {
        let exempt = |kinds: &'static [Kind], rows| Some(Exempt { kinds, columns: &["p"], rows });
        let register = [
            Divergence {
                id: "DIV-0101",
                fluids: &["A"],
                exempt: exempt(&[Kind::Checkpoints], Rows::TBand { lo: 1.0, hi: 2.0 }),
                ..DIVERGENCES[5]
            },
            Divergence { id: "DIV-0102", fluids: &["*"], exempt: exempt(&[Kind::Sat], Rows::All), ..DIVERGENCES[5] },
            Divergence {
                id: "DIV-0103",
                fluids: &["*"],
                exempt: exempt(&[Kind::Props], Rows::TwoPhase),
                ..DIVERGENCES[5]
            },
        ];
        let at = |fluid, kind, column, t| exempt_at(&register, fluid, kind, column, t);
        assert_eq!([1.0, 1.5, 2.0].map(|t| at("A", Kind::Checkpoints, "p", t)), [Some("DIV-0101"); 3]);
        assert_eq!([0.5, 2.5].map(|t| at("A", Kind::Checkpoints, "p", t)), [None; 2]);
        assert_eq!((at("A", Kind::Checkpoints, "rhoV", 1.5), at("B", Kind::Checkpoints, "p", 1.5)), (None, None));
        assert_eq!(at("B", Kind::Sat, "p", 9.0), Some("DIV-0102"));
        assert_eq!(at("B", Kind::Props, "p", 9.0), None, "two-phase rows need more than T");
        let pg = |column, t| exempt_at(DIVERGENCES, "PropyleneGlycol", Kind::Checkpoints, column, t);
        assert_eq!(
            [pg("rhoV", 213.0), pg("p", 227.6028), pg("rhoL", 213.0), pg("p", 228.0)],
            [Some("DIV-0016"), Some("DIV-0016"), None, None]
        );
        // DIV-0018: the oracle's PQ cells but p, its input; not its QT or `sa` rows, nor a row whose input is unknown.
        let sat = |column, input| exempt_row(DIVERGENCES, "Water", Kind::Sat, column, 400.0, input);
        assert_eq!(
            [sat("T", Some("p")), sat("sV", Some("p")), sat("p", Some("p")), sat("T", Some("T")), sat("T", None)],
            [Some("DIV-0018"), Some("DIV-0018"), None, None, None]
        );
        assert_eq!(exempt_at(DIVERGENCES, "Water", Kind::Sat, "rhoL", 400.0), None);
    }
}
