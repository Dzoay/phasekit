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
    /// Rows outside the model's domain.
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
    /// Two-phase rows with lo ≤ T ≤ hi (a `flash` row's state in the dome).
    TwoPhaseTBand {
        /// Lower temperature (K).
        lo: f64,
        /// Upper temperature (K).
        hi: f64,
    },
    /// The `flash` kind's PT rows (input pair `PT`).
    PtInputs,
}

/// What a row is known by when the register is asked about it: its temperature, the input label of a `sat` row or the
/// pair of a `flash` row, and whether its state is two-phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowKey<'a> {
    /// The row's temperature (K).
    pub t: f64,
    /// The `sat` kind's input label (`T`, `p` or `sa`) or the `flash` kind's pair (`DT`, `PT`), if any.
    pub input: Option<&'a str>,
    /// Whether the row's state is two-phase.
    pub two_phase: bool,
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
    /// Measured bounds of single tables where they differ from `tolerance`: (the table's fixture path, its bound).
    pub table_tolerances: &'static [(&'static str, Tolerance)],
    /// Milestones whose PRs add the proof's parts, e.g. `&[5, 6]`.
    pub proof: &'static [u8],
    /// Open | ResolvedUpstream { commit }.
    pub status: DivStatus,
}

/// The register (VERIFICATION.md §6), seeded from map 12 §6.3 and the map 10 §8.5 seed list (E15); it grows by PR
/// with a proof per entry. `Investigate` stubs become `UsePaper` once their arbiter row is transcribed
/// (M1-M8), or `KeepOracle` when the user accepts the oracle value (DIV-0005, docs/DECISIONS.md).
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        evidence: "map 13 §3 item 4, R3: R 8.3144598 kept; IR 8474 Table 3 fails with its Table 1 R; tolerance 5e-7; \
                   M6.10: Table 4 fails too (12 cells, 3.5e-6), the shipped model within 1.05e-6 of it, tolerance 2e-6",
        exempt: None,
        tolerance: Some(Tolerance::Relative(5e-7)),
        table_tolerances: &[("paper/Helium/OrtizVega-JPCRD-2019.4.csv", Tolerance::Relative(2e-6))],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
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
        table_tolerances: &[],
        proof: &[6],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0019",
        fluids: &["SES36"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::None,
        evidence: "map 04 U4, M6.9: at SES36's lowest temperatures (p 114 Pa at 206.27 K) CoolProp's in-dome DT p changes \
                   with Q at fixed T, weighting its VLE sides' pressures (1.3e-9 apart), and sits 1e-8 from phasekit's VLE, \
                   whose g' = g'' to 1e-15",
        exempt: Some(Exempt {
            kinds: &[Kind::Flash],
            columns: &["p"],
            rows: Rows::TwoPhaseTBand { lo: 200.0, hi: 220.0 },
        }),
        tolerance: None,
        table_tolerances: &[],
        proof: &[6],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0020",
        fluids: &["*"],
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::Code("phasekit_core::flash"),
        evidence: "map 03 §6 row 8 (ROT-074), M7.1: CoolProp's PT reports p, h, s and u at its solver's last iterate, not \
                   at the density it returns: Nitrogen at 107.45 MPa and 1192.98 K, p 7.9e-9 off its input and off its \
                   own DT at the same (T, rho), which reproduces the input to 2.2e-16; Air at 685.58 MPa and 1514.94 K, \
                   h 6.1e-10 off its DT; on the nightly's 40 x 40 grids h, s and u up to 4.0e-9, 1.0e-9 and 6.3e-9 off \
                   (Argon, Air; 2026-10-10); its rho is the converged root, within Flash; phasekit's PT state is built \
                   at the density its solve returned, its p the input to rounding",
        exempt: Some(Exempt { kinds: &[Kind::Flash], columns: &["p", "h", "s", "u"], rows: Rows::PtInputs }),
        tolerance: None,
        table_tolerances: &[],
        proof: &[7],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0021",
        fluids: &["SulfurHexafluoride", "R125"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.1, M8.5: printed check values off their own correlations, which v8.0.0 transcribes \
                   exactly: Quinones-Cisneros et al. 2012 Table 8 at rho = 0, 15.2887 and 19.6796 uPa s, against its \
                   Eq. (6) with its Table 2, 15.288107 and 19.678983 (-3.9e-5 at 300 K, -3.1e-5 at 400 K); Huber & \
                   Laesecke 2006 at 300 K and 10.5969998 mol/L, 177.37 against 177.38653 (+9.3e-5), its 400 K point \
                   within its digits (2026-10-12)",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/SulfurHexafluoride/CoolProp-Tests.eta.csv", Tolerance::Relative(4e-5)),
            ("paper/R125/CoolProp-Tests.eta.csv", Tolerance::Relative(1e-4)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0022",
        fluids: &["Ammonia", "R123", "n-Octane", "n-Propane"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.1, M8.5: CoolProp-Tests prints these viscosity rows' densities to fewer digits than eta \
                   needs, and each printed eta lies within eta(rho +- half a printed unit of rho) +- half its own: \
                   Ammonia at 200 K and 42754.4 mol/m3 -1.1e-5, R123 at 265 K and 1545.8 kg/m3 +1.15e-4, n-Octane at \
                   300 K and 6177.2 mol/m3 -4.9e-5, n-Propane at 90 to 600 K and 4-digit densities up to -3.3e-3 \
                   (2026-10-12); to transcribe: the papers' own tables",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/Ammonia/CoolProp-Tests.eta.csv", Tolerance::Relative(2e-5)),
            ("paper/R123/CoolProp-Tests.eta.csv", Tolerance::Relative(2e-4)),
            ("paper/n-Octane/CoolProp-Tests.eta.csv", Tolerance::Relative(5e-5)),
            ("paper/n-Propane/CoolProp-Tests.eta.csv", Tolerance::Relative(4e-3)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0023",
        fluids: &["Air", "Argon", "Nitrogen", "Oxygen", "SulfurHexafluoride", "n-Nonane"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.1, M8.6: conductivity check values off their printed digits, within CoolProp's 1e-4: \
                   Lemmon & Jacobsen 2004's points near the critical point, Air -4.5e-6, Argon -6.8e-6, Nitrogen \
                   -5.4e-6, Oxygen +1.2e-5, where lambda moves by 1e-5 to 2e-3 over half a printed unit of rho; \
                   Assael et al. 2012's SF6 at 310 K (0.97 Tc) and 1200 kg/m3, -1.4e-5; Huber 2005's n-Nonane at \
                   300 K and 5619.4 mol/m3, +5.0e-5, within lambda(rho +- half a printed unit) (2026-10-12); to \
                   transcribe: the papers' own tables",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/Air/CoolProp-Tests.lambda.csv", Tolerance::Relative(5e-6)),
            ("paper/Argon/CoolProp-Tests.lambda.csv", Tolerance::Relative(7e-6)),
            ("paper/Nitrogen/CoolProp-Tests.lambda.csv", Tolerance::Relative(6e-6)),
            ("paper/Oxygen/CoolProp-Tests.lambda.csv", Tolerance::Relative(2e-5)),
            ("paper/SulfurHexafluoride/CoolProp-Tests.lambda.csv", Tolerance::Relative(2e-5)),
            ("paper/n-Nonane/CoolProp-Tests.lambda.csv", Tolerance::Relative(6e-5)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0024",
        fluids: &["Methane"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 05 R10, map 10 §8.1, M8.6: Friend, Ely & Ingham 1989 computed their check values with their own \
                   equation of state; CoolProp's hard-coded model reads Setzmann & Wagner's ideal-gas cp and states: \
                   400 K dilute -1.4e-4, the saturated liquid at 182 K +1.2e-3, 100 K and 28.8e3 mol/m3 (rho printed to \
                   3 digits) -8.8e-3 (2026-10-12); to transcribe: the paper's table, with its equation of state",
        exempt: None,
        tolerance: None,
        table_tolerances: &[("paper/Methane/CoolProp-Tests.lambda.csv", Tolerance::Relative(9e-3))],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0025",
        fluids: &["n-Pentane"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 05 R6, map 10 §8.1, M8.6: Vassiliou et al. 2015's check point at 460 K and 377.687 kg/m3, \
                   71.300 mW/m/K, is +4.75e-2 off: the dilute (34.048) and residual (33.325) terms agree to their \
                   digits, the critical term is 7.313 against 3.927 because CoolProp hands it n-Pentane's friction-theory \
                   viscosity, 27.15 uPa s, where the correlation was fitted with an ECS one, 49.465 uPa s; with that, \
                   4.014 (2026-10-12); to decide: the viscosity the term reads, and n-Pentane's viscosity itself \
                   (Quinones-Cisneros & Deiters 2006)",
        exempt: None,
        tolerance: None,
        table_tolerances: &[("paper/n-Pentane/CoolProp-Tests.lambda.csv", Tolerance::Relative(5e-2))],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0026",
        fluids: &["R13", "R14", "R142b", "R218", "RC318"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::Code("phasekit_core::transport::ecs"),
        evidence: "map 05 R2, M8.8: CoolProp's parse_ECS_conductivity (FluidLibrary.h:668-680) never reads an ECS \
                   conductivity's q_D, so its Olchowy-Sengers term keeps the default 2e9 1/m; v8.0.0's files give R13 \
                   2.86e9, R14 4.41e9, R142b 1.62e9, R218 0.917e9 and RC318 2.81e9 (the other 13 ECS fluids 2e9); \
                   phasekit reads them, which moves lambda at the oracle's states by up to +1.9e-2 (R13), +1.8e-1 \
                   (R14), -2.0e-3 (R142b), -3.2e-3 (R218) and +1.1e-1 (RC318), and near Tc by -16 % to +24 % \
                   (map 05 R2; 2026-10-12); to transcribe: Huber, Laesecke and Perkins, IECR 2003, the five blocks' \
                   source",
        exempt: Some(Exempt { kinds: &[Kind::Transport], columns: &["lambda"], rows: Rows::All }),
        tolerance: None,
        table_tolerances: &[],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0027",
        fluids: &["EthylBenzene", "o-Xylene"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.1, M8.8: Mylona et al. 2014 (JPCRD 43, 043104) Table 21's 0 MPa row misses the \
                   paper's own dilute-gas fit, Eq. (18), which v8.0.0 transcribes exactly (Tc = 617.12 K): at 500 and \
                   600 K Eq. (18) gives 29.1547 and 41.1347 mW/(m K) against the printed 29.16 and 41.14 (-1.8e-4, \
                   -1.3e-4), the other four temperatures within their digits; CoolProp-Tests.cpp:555 takes the 600 K \
                   value; its Table 6 likewise prints o-xylene's 0 MPa, 300 K value as 13.68 where its Eq. (12), \
                   transcribed exactly, gives 13.6747 (-3.9e-4) and its Eqs. (3)-(5) and (11) 13.684 \
                   (CoolProp-Tests.cpp:548) (2026-10-12); to decide: what the tables were computed with (the paper's \
                   dilute-gas scheme, within 0.1 % of its fits)",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/EthylBenzene/CoolProp-Tests.lambda.csv", Tolerance::Relative(2e-4)),
            ("paper/o-Xylene/CoolProp-Tests.lambda.csv", Tolerance::Relative(4e-4)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0028",
        fluids: &["R1234yf"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 05 R4, M8.9: a rhosr-CS block's (rho s_r)_c is rho_r R (tau a^r_tau - a^r) at its EOS's reducing \
                   state; six of v8.0.0's seven match their EOS within 2e-6, but R1234yf's stored -54468.48 is 6.9e-3 \
                   from its current EOS's -54095.50 (its EOS was replaced after the value was derived); with the \
                   derived value its saturated-liquid viscosity rises +1.6e-2 at 250 K and +7.8e-3 at 340 K, its vapour \
                   below 7e-4 (2026-10-12); both data sets keep the stored value, as CoolProp does; to decide: derive \
                   it from the EOS",
        exempt: None,
        tolerance: None,
        table_tolerances: &[],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0029",
        fluids: &["R124", "R22", "R245fa", "R32"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 05 R17, M8.9: these four have a rhosr-CS viscosity, whose block has no Lennard-Jones parameters, \
                   and an ECS conductivity, whose dilute terms CoolProp evaluates with the Chung estimate it fills in \
                   from the EOS reducing state (named LjSource::Chung in the record); the parameters that ship with the \
                   same ECS model, in each file's unread second viscosity entry (R245fa sigma 0.5529 nm against Chung's \
                   0.5151), lower the dilute-gas conductivity at 250-400 K by up to 1.1e-1 (R124), 8.3e-2 (R22), 1.2e-1 \
                   (R245fa) and 5.7e-2 (R32) (2026-10-12); to transcribe: Huber, Laesecke and Perkins 2003, Klein et \
                   al. 1997 and McLinden et al. 2000",
        exempt: None,
        tolerance: None,
        table_tolerances: &[],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0030",
        fluids: &["Ammonia"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::SkipOracle,
        fix: Fix::Code("phasekit_core::transport::hardcoded"),
        evidence: "map 05 R3, M8.9: Tufeu et al. 1984's critical conductivity term, as CoolProp codes it, diverges at \
                   its own Tc, 405.4 K, at every density: the oracle gives 6.26 W/m/K at 405.3999 K and 50 kg/m3, \
                   about 100 times the conductivity there, and NaN at 405.4 K; phasekit floors the term's reduced \
                   temperature at the gap to the EOS's Tc, 405.56 K, so it is CoolProp's outside 405.24-405.56 K and \
                   finite and continuous inside (2026-10-12); the Corrected model (Monogenidou et al. 2018, or a bound \
                   from Tufeu's paper) waits for the paper check (map 05 Q3)",
        exempt: Some(Exempt {
            kinds: &[Kind::Transport],
            columns: &["lambda"],
            rows: Rows::TBand { lo: 405.24, hi: 405.56 },
        }),
        tolerance: None,
        table_tolerances: &[],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0031",
        fluids: &["Benzene", "n-Heptane", "Methanol"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.1, M8.9: viscosity check values off their papers' own correlations, which v8.0.0 \
                   transcribes exactly, at the tables' own temperatures and densities: Avgeri et al. 2014 Table 8 at \
                   300 K and 875 kg/m3, 608.52 against 608.5308 uPa s (+1.8e-5); Michailidou et al. 2014 Table 7 at \
                   250 K and 720 kg/m3 and at 550 K and 500 kg/m3, 725.69 and 95.105 against 725.614 and 95.1024 \
                   (-1.0e-4, -2.7e-5); Xiang et al. 2006 Table 6 at 300 K and 788.41 kg/m3 (-1.3e-4) and at 630 K and \
                   888.50 kg/m3 (-1.46e-2), whose 630 K column at 500-800 MPa implies densities about 5 kg/m3 above the \
                   printed ones while its 620 K column agrees to 1e-4 (2026-10-12); to decide: with the papers' authors",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/Benzene/CoolProp-Tests.eta.csv", Tolerance::Relative(2e-5)),
            ("paper/n-Heptane/CoolProp-Tests.eta.csv", Tolerance::Relative(2e-4)),
            ("paper/Methanol/CoolProp-Tests.eta.csv", Tolerance::Relative(2e-2)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0032",
        fluids: &["Cyclopentane", "Isopentane", "n-Heptane", "n-Hexane"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 05 R6, map 10 §8.1, M8.9: conductivity check values whose critical terms the papers evaluated with \
                   another viscosity and equation of state than v8.0.0 hands them (DIV-0025's cause): Vassiliou et al. \
                   2015 at 512 K and 400 kg/m3 for cyclopentane, 69.698 mW/m/K with an ECS viscosity of 40.842 uPa s \
                   where CoolProp's Chung model reads 47.82 (-1.81e-2), and at 460 K and 329.914 kg/m3 for isopentane, \
                   59.649 with REFPROP 9.1's 36.170 uPa s where CoolProp reads 37.20 (-4.4e-3); Assael et al. 2013 at 535 K and 100 \
                   kg/m3 for n-heptane, 51.655 with 15.253 uPa s where CoolProp reads Michailidou 2014's 15.726 \
                   (-2.9e-3), and its other n-heptane and n-hexane points (-5.8e-5, -4.7e-5, -2.3e-4), where CoolProp's \
                   critical term is small or zero and the papers' used Span-Wagner's EOS; n-hexane's q_D is 1.35e9 1/m in \
                   v8.0.0 against the paper's 1/(7.37e-10 m), which does not move its point (2026-10-12); to decide: \
                   which viscosity the critical term reads (map 05 R6's declared viscosity)",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/Cyclopentane/CoolProp-Tests.lambda.csv", Tolerance::Relative(2e-2)),
            ("paper/Isopentane/CoolProp-Tests.lambda.csv", Tolerance::Relative(5e-3)),
            ("paper/n-Heptane/CoolProp-Tests.lambda.csv", Tolerance::Relative(3e-3)),
            ("paper/n-Hexane/CoolProp-Tests.lambda.csv", Tolerance::Relative(3e-4)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0033",
        fluids: &["Ethane"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.1, M8.9: Friend, Ingham and Ely 1991's Table A3, from which CoolProp-Tests.cpp:211-212 and \
                   455-456 come, is indexed by T and p, its densities from the paper's own equation of state printed to \
                   0.01 mol/dm3: CoolProp's rows take those rounded densities (and 878.68 uPa s as 878.6), off by \
                   +1.7e-3 and +3.8e-4 in viscosity; its conductivity points, -4.3e-3 and +3.3e-3, have a critical \
                   term with the paper's own equation of state and constants (A = 1.01 against CoolProp's R0 = 1.03, \
                   a reference temperature of 2 Tc against 1.5 Tc); and the paper's Table 9 prints the higher-order \
                   viscosity coefficient g9 as -0.030425255 where CoolProp hard-codes -0.030435255 \
                   (TransportRoutines.cpp:617) (2026-10-12); to decide: the paper's g9 under Corrected, and Friend's \
                   critical-term constants",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/Ethane/CoolProp-Tests.eta.csv", Tolerance::Relative(2e-3)),
            ("paper/Ethane/CoolProp-Tests.lambda.csv", Tolerance::Relative(5e-3)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0034",
        fluids: &["R23", "CarbonDioxide", "R123"],
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        fix: Fix::None,
        evidence: "map 10 §8.1, M8.9: check values off their printed digits whose papers are not open access: Shan, \
                   Penoncello and Jacobsen 2000's R23 points (eta -8.9e-5, +6.4e-5; lambda -7.7e-5, +1.08e-4), each \
                   within its value at the row's density plus or minus half a printed unit (CoolProp gives mol/m3 to \
                   five digits); Laesecke and Muzny 2017's CO2 point at 300 K and 1400 kg/m3 (+1.05e-5); Laesecke et \
                   al. 1995's R123 point at 180 K and 0.002873 kg/m3 (-9.6e-4) (2026-10-12); to transcribe: the papers' \
                   own tables",
        exempt: None,
        tolerance: None,
        table_tolerances: &[
            ("paper/R23/CoolProp-Tests.eta.csv", Tolerance::Relative(9e-5)),
            ("paper/R23/CoolProp-Tests.lambda.csv", Tolerance::Relative(2e-4)),
            ("paper/CarbonDioxide/CoolProp-Tests.eta.csv", Tolerance::Relative(2e-5)),
            ("paper/R123/CoolProp-Tests.lambda.csv", Tolerance::Relative(1e-3)),
        ],
        proof: &[8],
        status: DivStatus::Open,
    },
];

/// The register entry, if any, whose `exempt` cells include `column` of a `kind` row of `fluid` at temperature `t`: an
/// exemption of every row or of a temperature band (the others need more than T to decide).
pub fn exempt_at(register: &[Divergence], fluid: &str, kind: Kind, column: &str, t: f64) -> Option<&'static str> {
    exempt_row(register, fluid, kind, column, RowKey { t, input: None, two_phase: false })
}

/// [`exempt_at`] for a row known by more than its T ([`RowKey`]): [`Rows::GivenP`] exempts the rows given p,
/// [`Rows::TwoPhaseTBand`] the two-phase rows of a temperature band and [`Rows::PtInputs`] the PT rows.
pub fn exempt_row(
    register: &[Divergence],
    fluid: &str,
    kind: Kind,
    column: &str,
    row: RowKey<'_>,
) -> Option<&'static str> {
    let RowKey { t, input, two_phase } = row;
    let applies = |d: &&Divergence| {
        let Some(e) = &d.exempt else { return false };
        let rows = match e.rows {
            Rows::All => true,
            Rows::TBand { lo, hi } => lo <= t && t <= hi,
            Rows::GivenP => input == Some("p"),
            Rows::TwoPhaseTBand { lo, hi } => two_phase && lo <= t && t <= hi,
            Rows::PtInputs => input == Some("PT"),
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
    /// DIV-0018 (through `exempt_row`) the PQ rows of the `sat` kind, DIV-0019 the two-phase `flash` rows of a T band and
    /// DIV-0020 the `flash` kind's PT rows.
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
        let sat = |column, input| {
            exempt_row(DIVERGENCES, "Water", Kind::Sat, column, RowKey { t: 400.0, input, two_phase: false })
        };
        assert_eq!(
            [sat("T", Some("p")), sat("sV", Some("p")), sat("p", Some("p")), sat("T", Some("T")), sat("T", None)],
            [Some("DIV-0018"), Some("DIV-0018"), None, None, None]
        );
        assert_eq!(exempt_at(DIVERGENCES, "Water", Kind::Sat, "rhoL", 400.0), None);
        // DIV-0019: SES36's two-phase flash rows from 200 K to 220 K, their p only.
        let ses36 = |column, t, two_phase| {
            exempt_row(DIVERGENCES, "SES36", Kind::Flash, column, RowKey { t, input: None, two_phase })
        };
        assert_eq!(
            [ses36("p", 206.0, true), ses36("p", 220.0, true), ses36("p", 206.0, false), ses36("h", 206.0, true)],
            [Some("DIV-0019"), Some("DIV-0019"), None, None]
        );
        assert_eq!((ses36("p", 199.9, true), ses36("p", 220.1, true)), (None, None));
        // DIV-0020: p, h, s and u of every fluid's PT rows, not of its DT rows, nor ρ.
        let flash = |column, input| {
            exempt_row(DIVERGENCES, "Water", Kind::Flash, column, RowKey { t: 300.0, input, two_phase: false })
        };
        assert_eq!(
            [flash("p", Some("PT")), flash("u", Some("PT")), flash("rho", Some("PT")), flash("p", Some("DT"))],
            [Some("DIV-0020"), Some("DIV-0020"), None, None]
        );
        assert_eq!(flash("h", None), None);
    }
}
