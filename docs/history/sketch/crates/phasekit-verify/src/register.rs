//! The compiled divergence register: every deliberate difference from the CoolProp 8.0.0 oracle is a typed,
//! cited entry, cross-checked against the data corrections and proved by a three-part test
//! (docs/ROT-REGISTER.md, forthcoming). Literature arbitration as executable TDD.

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

/// What the tests do about it.
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

/// One register entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// Stable id cited by patches, fixtures and `#[ignore = "DIV-…"]`.
    pub id: &'static str,
    /// Fluid (canonical name) or `"*"`.
    pub fluid: &'static str,
    /// Affected part.
    pub part: Part,
    /// The arbiter (BibTeX key and table), required for `UsePaper`.
    pub arbiter: Option<&'static str>,
    /// Policy.
    pub policy: Policy,
    /// Where the evidence lives.
    pub evidence: &'static str,
    /// Lifecycle.
    pub status: DivStatus,
}

/// The register, seeded from map 12 §6.3 and the map 10 §8.5 seed list (E15); grows by PR with a proof
/// test per entry. `Investigate` stubs become `UsePaper` once their arbiter row is transcribed (M1-M8), or
/// `KeepOracle` when the user accepts the oracle value (DIV-0005, docs/design/04-user-decisions.md).
pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "DIV-0001",
        fluid: "R1234ze(E)",
        part: Part::GasConstant,
        arbiter: Some("Thol-IJT-2016-R1234zeE Table 3"),
        policy: Policy::UsePaper,
        evidence: "map 13 §3 item 3: stored R 8.314472, paper R 8.3144621",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0002",
        fluid: "Water",
        part: Part::Melting,
        arbiter: Some("IAPWS R14-08"),
        policy: Policy::UsePaper,
        evidence: "map 10 R10: ice VI p0 623.4 MPa vs 632.4 MPa",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0003",
        fluid: "Nitrogen",
        part: Part::Reducing,
        arbiter: Some("Span-JPCRD-2000"),
        policy: Policy::UsePaper,
        evidence: "map 12 §6.3: rho_r 11183.901464580624 vs 11183.9 (upstream 2acbbc82)",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0004",
        fluid: "*",
        part: Part::Transport,
        arbiter: None,
        policy: Policy::SkipOracle,
        evidence: "map 05 §6, map 10 R18: two-phase viscosity/conductivity are meaningless",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0005",
        fluid: "Helium",
        part: Part::GasConstant,
        arbiter: None,
        policy: Policy::KeepOracle,
        evidence: "map 13 §3.4, R3: R 8.3144598 kept; IR 8474 Table 3 fails with its Table 1 R; tolerance 5e-7",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0006",
        fluid: "Ethylene",
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        evidence: "map 12 §6.3: reducing density (and M) corrected upstream 2acbbc82; arbiter row to transcribe",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0007",
        fluid: "OrthoHydrogen",
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        evidence: "map 12 §6.3: reducing density (and M) corrected upstream 2acbbc82; arbiter row to transcribe",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0008",
        fluid: "n-Undecane",
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        evidence: "map 12 §6.3: reducing density corrected upstream 2acbbc82; arbiter row to transcribe",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0009",
        fluid: "R1233zd(E)",
        part: Part::Transport,
        arbiter: None,
        policy: Policy::Investigate,
        evidence: "map 12 §6.3: v8.0.0 raises 'Viscosity model is not available'; restored upstream 14da1f0d",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0010",
        fluid: "*",
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        evidence: "map 12 §6.3: PR/SRK entropy inconsistent (T(ds/dT)p 91.35 vs cp 93.89); identity arbitrates",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0011",
        fluid: "*",
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        evidence: "map 12 §6.3, R8: Cvirial from delta = 1e-12 is off by up to 7.1e-5; exact Taylor path (E4)",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0012",
        fluid: "Water",
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::SkipOracle,
        evidence: "map 12 §6.3: DT at 250 K < Tmin gives p = -5.928 Pa without error; we refuse (DomainError)",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0013",
        fluid: "R1234yf&R1234ze(E)",
        part: Part::Reducing,
        arbiter: None,
        policy: Policy::Investigate,
        evidence: "map 10 §8.4: mixture alphar -0.464679 vs Bell 2022 Table XI -0.460595; reducing T differs (M13)",
        status: DivStatus::Open,
    },
    Divergence {
        id: "DIV-0014",
        fluid: "R1224YDZ",
        part: Part::Algorithm,
        arbiter: None,
        policy: Policy::Investigate,
        evidence: "map 10 §8.4: p(400 K, 8000 mol/m3) 21.1790735 MPa vs printed 21.17909 (3.3x half a digit)",
        status: DivStatus::Open,
    },
];

/// Why the register and the corrections disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegisterError {
    /// A patch cites an id that is not in the register.
    UnknownId(Box<str>),
    /// A patch cites an entry whose policy is not `UsePaper`, or that has no arbiter.
    NotUsePaper(Box<str>),
    /// Two entries share an id.
    DuplicateId(&'static str),
}

/// Cross-checks the register against the patches shipped in the data (the real test decodes every embedded
/// record with `Registry::record` and passes all of their patches).
pub fn check_register(register: &[Divergence], patches: &[Patch]) -> Result<(), RegisterError> {
    for (i, d) in register.iter().enumerate() {
        if register[..i].iter().any(|e| e.id == d.id) {
            return Err(RegisterError::DuplicateId(d.id));
        }
    }
    for p in patches {
        let entry = register.iter().find(|d| *d.id == *p.divergence);
        match entry {
            None => return Err(RegisterError::UnknownId(p.divergence.clone())),
            Some(d) if d.policy != Policy::UsePaper || d.arbiter.is_none() => {
                return Err(RegisterError::NotUsePaper(p.divergence.clone()));
            }
            Some(_) => {}
        }
    }
    Ok(())
}
