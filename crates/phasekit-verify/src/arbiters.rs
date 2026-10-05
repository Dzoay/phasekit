//! The literature arbiters (VERIFICATION.md §4.3, §4.4; map 13 A1): one record per (fluid, part) with its citation,
//! tables, printed constants and status. Only a `SelfConsistent` table overrules the oracle, and the status is asserted
//! (`tests/arbiters.rs`), so an arbiter cannot change category silently.

/// What the arbiter checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum ArbiterPart {
    AlphaR,
    Alpha0,
    Viscosity,
    Conductivity,
    SurfaceTension,
    Melting,
    Saturation,
}

/// The kind of a printed table (map 13 §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableKind {
    /// α and its derivatives at a (T, ρ) state.
    K1,
    /// Properties at (T, ρ) states.
    K2,
    /// Saturation states.
    K3,
    /// Property rows (transport, σ, melting).
    Rows,
}

/// Who publishes the arbiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Role {
    /// An IAPWS release.
    Release,
    /// The equation-of-state paper.
    EosPaper,
    /// A property-model paper (transport, melting).
    PropertyPaper,
    /// Rows of CoolProp's own tests traced to a paper (map 10 §8.1).
    CoolPropTests,
}

/// Where the arbiter is published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Citation {
    /// Bibliography key (map 13).
    pub key: &'static str,
    /// DOI ("10.…") or report ("IAPWS R6-95(2018)", "NIST IR 8474"); `None` only for unpublished models.
    pub doi_or_report: Option<&'static str>,
    /// Publisher's role.
    pub role: Role,
}

/// One printed table, transcribed to `crates/phasekit-verify/fixtures/<file>` (VERIFICATION.md §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Table {
    /// `paper/<Fluid>/<bibkey>.<table>.csv`.
    pub file: &'static str,
    /// Kind.
    pub kind: TableKind,
    /// Rows, where VERIFICATION.md §4.4 states them before transcription.
    pub rows: Option<u16>,
}

/// The constants a paper prints, recorded at transcription as printed, with their units (VERIFICATION.md §4.2 step
/// 5): a mass-based release such as IAPWS-95 prints a specific R and no molar mass, so nothing is converted here; the
/// evaluation (M4 on) converts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Constants {
    /// Gas constant, e.g. "0.46151805 kJ/(kg K)".
    pub r: &'static str,
    /// Molar mass, where the paper prints one.
    pub molar_mass: Option<&'static str>,
    /// Reducing temperature, e.g. "647.096 K".
    pub t_reducing: &'static str,
    /// Reducing density, e.g. "322 kg/m3".
    pub rho_reducing: &'static str,
}

/// IAPWS-95's printed constants (IAPWS R6-95(2018) Eqs. 1-3; T_r = T_c, ρ_r = ρ_c).
const IAPWS_95: Constants =
    Constants { r: "0.46151805 kJ/(kg K)", molar_mass: None, t_reducing: "647.096 K", rho_reducing: "322 kg/m3" };

/// The constants Lemmon et al. (J. Chem. Eng. Data 60:3745, 2015) print for each fluid (§2.4, Appendix A; δ = ρ/ρ_c,
/// τ = T_c/T, so T_r = T_c and ρ_r = ρ_c). R is stated once for the paper (Eq. 5) and again in the R-115 block.
const fn lemmon_2015(molar_mass: &'static str, t_reducing: &'static str, rho_reducing: &'static str) -> Constants {
    Constants { r: "8.3144621 J/(mol K)", molar_mass: Some(molar_mass), t_reducing, rho_reducing }
}
const LEMMON_R227EA: Constants = lemmon_2015("170.02886 g/mol", "374.9 K", "3.495 mol/dm3");
const LEMMON_R365MFC: Constants = lemmon_2015("148.07452 g/mol", "460.0 K", "3.2 mol/dm3");
const LEMMON_R115: Constants = lemmon_2015("154.466416 g/mol", "353.1 K", "3.98 mol/dm3");
const LEMMON_R13I1: Constants = lemmon_2015("195.9104 g/mol", "396.44 K", "4.4306 mol/dm3");

/// The constants Thol and Lemmon (Int. J. Thermophys. 37:28, 2016) print for R-1234ze(E) (§2; δ = ρ/ρ_c, τ = T_c/T).
const THOL_R1234ZEE: Constants = Constants {
    r: "8.3144621 J/(mol K)",
    molar_mass: Some("114.0416 g/mol"),
    t_reducing: "382.513 K",
    rho_reducing: "4.29 mol/dm3",
};

/// Helium's constants as NIST IR 8474 Table 1 prints them (τ = T_c/T, δ = ρ/ρ_c). The report says to use this R
/// (8.314472), but its own Table 3 does not reproduce with it, and CoolProp stores 8.3144598 (DIV-0005; map 13 §3
/// item 4, R3); Table 2's coefficients match CoolProp's term for term.
const NIST_IR_8474: Constants = Constants {
    r: "8.314472 J/(mol K)",
    molar_mass: Some("4.002602 g/mol"),
    t_reducing: "5.1953 K",
    rho_reducing: "17.3837 mol/dm3",
};

/// Where an arbiter stands (VERIFICATION.md §4.3).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ArbiterStatus {
    /// A table is expected, not yet obtained.
    Expected,
    /// Double-entered, not yet evaluated.
    Transcribed,
    /// Every row within `Paper` with the paper's own constants: the only status that arbitrates.
    SelfConsistent,
    /// No constant set stated in the paper reproduces it.
    Inconsistent {
        /// The largest relative residual found.
        residual: f64,
    },
    /// The paper states no table.
    None,
    /// No paper exists (Propylene, SES36, Neon).
    Unpublished,
}

/// One arbiter record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arbiter {
    /// Canonical fluid name.
    pub fluid: &'static str,
    /// What it checks.
    pub part: ArbiterPart,
    /// Where it is published.
    pub citation: Citation,
    /// Its tables.
    pub tables: &'static [Table],
    /// The paper's constants, once transcribed.
    pub constants: Option<Constants>,
    /// Its status.
    pub status: ArbiterStatus,
}

/// The milestone whose step first evaluates a transcribed table (PLAN.md M4.5, the IAPWS-95 α table).
pub const EVALUATED_FROM: u8 = 4;

/// The status rules a record breaks, given which fixture files are committed and the first open milestone: an
/// `Expected` record has no committed table, a `Transcribed` one has all of them, `None` and `Unpublished` have none to
/// list, an evaluated status needs the evaluation (M4 on), every record but an unpublished one cites a DOI or report,
/// and no (fluid, part) appears twice.
pub fn violations(arbiters: &[Arbiter], committed: &dyn Fn(&str) -> bool, milestone: u8) -> Vec<String> {
    let mut errors = Vec::new();
    for (i, a) in arbiters.iter().enumerate() {
        let name = format!("{} {:?}", a.fluid, a.part);
        if arbiters[..i].iter().any(|b| b.fluid == a.fluid && b.part == a.part) {
            errors.push(format!("{name}: listed twice"));
        }
        let source = a.citation.doi_or_report.unwrap_or_default();
        let cited = source.starts_with("10.") || source.starts_with("IAPWS ") || source.starts_with("NIST IR ");
        if a.status != ArbiterStatus::Unpublished && !cited {
            errors.push(format!("{name}: cites no DOI or report (`{source}`)"));
        }
        let any = a.tables.iter().any(|t| committed(t.file));
        let all = !a.tables.is_empty() && a.tables.iter().all(|t| committed(t.file));
        let evaluated = matches!(a.status, ArbiterStatus::SelfConsistent | ArbiterStatus::Inconsistent { .. });
        match a.status {
            ArbiterStatus::Expected if any => {
                errors.push(format!("{name}: a table is committed, so it is Transcribed"))
            }
            ArbiterStatus::Transcribed if !all => {
                errors.push(format!("{name}: Transcribed, but not every table is committed"))
            }
            _ if evaluated && !all => errors.push(format!("{name}: {:?}, but not every table is committed", a.status)),
            _ if evaluated && milestone <= EVALUATED_FROM => {
                errors.push(format!("{name}: {:?}, but nothing is evaluated before M{EVALUATED_FROM} closes", a.status))
            }
            ArbiterStatus::None | ArbiterStatus::Unpublished if !a.tables.is_empty() => {
                errors.push(format!("{name}: {:?} lists no table", a.status))
            }
            _ => {}
        }
    }
    errors
}

/// Every arbiter record of VERIFICATION.md §4.4's core set; the 12 one-row CoolProp-test fluids join at M1.12, the
/// n-Heptane and D6 c_p⁰ equation checks with M4, the mp check points and transport rows with their files.
pub static ARBITERS: &[Arbiter] = &[
    Arbiter {
        fluid: "Water",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "IAPWS-R6-95-2018", doi_or_report: Some("IAPWS R6-95(2018)"), role: Role::Release },
        tables: &[
            Table { file: "paper/Water/IAPWS-R6-95-2018.6.csv", kind: TableKind::K1, rows: Some(1) },
            Table { file: "paper/Water/IAPWS-R6-95-2018.7.csv", kind: TableKind::K2, rows: Some(11) },
        ],
        constants: Some(IAPWS_95),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "Water",
        part: ArbiterPart::Saturation,
        citation: Citation { key: "IAPWS-R6-95-2018", doi_or_report: Some("IAPWS R6-95(2018)"), role: Role::Release },
        tables: &[Table { file: "paper/Water/IAPWS-R6-95-2018.8.csv", kind: TableKind::K3, rows: Some(3) }],
        constants: Some(IAPWS_95),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "Water",
        part: ArbiterPart::Viscosity,
        citation: Citation { key: "IAPWS-R12-08", doi_or_report: Some("IAPWS R12-08"), role: Role::Release },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "Water",
        part: ArbiterPart::Conductivity,
        citation: Citation { key: "IAPWS-R15-11", doi_or_report: Some("IAPWS R15-11"), role: Role::Release },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "Water",
        part: ArbiterPart::Melting,
        citation: Citation { key: "IAPWS-R14-08", doi_or_report: Some("IAPWS R14-08"), role: Role::Release },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "Water",
        part: ArbiterPart::SurfaceTension,
        citation: Citation { key: "IAPWS-R1-76-2014", doi_or_report: Some("IAPWS R1-76(2014)"), role: Role::Release },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "R227EA",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Lemmon-JCED-2016-365227",
            doi_or_report: Some("10.1021/acs.jced.5b00684"),
            role: Role::EosPaper,
        },
        tables: &[Table { file: "paper/R227EA/Lemmon-JCED-2016-365227.7.csv", kind: TableKind::K2, rows: Some(3) }],
        constants: Some(LEMMON_R227EA),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "R365MFC",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Lemmon-JCED-2016-365227",
            doi_or_report: Some("10.1021/acs.jced.5b00684"),
            role: Role::EosPaper,
        },
        tables: &[Table { file: "paper/R365MFC/Lemmon-JCED-2016-365227.7.csv", kind: TableKind::K2, rows: Some(3) }],
        constants: Some(LEMMON_R365MFC),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "R115",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Lemmon-JCED-2016-365227",
            doi_or_report: Some("10.1021/acs.jced.5b00684"),
            role: Role::EosPaper,
        },
        tables: &[Table { file: "paper/R115/Lemmon-JCED-2016-365227.7.csv", kind: TableKind::K2, rows: Some(3) }],
        constants: Some(LEMMON_R115),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "R13I1",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Lemmon-JCED-2016-365227",
            doi_or_report: Some("10.1021/acs.jced.5b00684"),
            role: Role::EosPaper,
        },
        tables: &[Table { file: "paper/R13I1/Lemmon-JCED-2016-365227.7.csv", kind: TableKind::K2, rows: Some(3) }],
        constants: Some(LEMMON_R13I1),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "R1234ze(E)",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Thol-IJT-2016-R1234zeE",
            doi_or_report: Some("10.1007/s10765-016-2040-6"),
            role: Role::EosPaper,
        },
        tables: &[Table { file: "paper/R1234ze(E)/Thol-IJT-2016-R1234zeE.3.csv", kind: TableKind::K2, rows: Some(6) }],
        constants: Some(THOL_R1234ZEE),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "Helium",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "OrtizVega-JPCRD-2019", doi_or_report: Some("NIST IR 8474"), role: Role::EosPaper },
        tables: &[Table { file: "paper/Helium/OrtizVega-JPCRD-2019.3.csv", kind: TableKind::K2, rows: Some(6) }],
        constants: Some(NIST_IR_8474),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "Helium",
        part: ArbiterPart::Saturation,
        citation: Citation { key: "OrtizVega-JPCRD-2019", doi_or_report: Some("NIST IR 8474"), role: Role::EosPaper },
        tables: &[Table { file: "paper/Helium/OrtizVega-JPCRD-2019.4.csv", kind: TableKind::K3, rows: Some(17) }],
        constants: Some(NIST_IR_8474),
        status: ArbiterStatus::Transcribed,
    },
    Arbiter {
        fluid: "R1234yf",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Lemmon-IJT-2022",
            doi_or_report: Some("10.1007/s10765-022-03015-y"),
            role: Role::CoolPropTests,
        },
        tables: &[Table { file: "paper/R1234yf/Lemmon-IJT-2022.7.csv", kind: TableKind::K2, rows: Some(6) }],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "R1130(E)",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Huber-IJT-2025-R1130E",
            doi_or_report: Some("10.1007/s10765-025-03535-3"),
            role: Role::CoolPropTests,
        },
        tables: &[Table { file: "paper/R1130(E)/Huber-IJT-2025-R1130E.4.csv", kind: TableKind::K2, rows: None }],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "R1224YDZ",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Akasaka-IJT-2023-R1224ydZ",
            doi_or_report: Some("10.1007/s10765-023-03266-3"),
            role: Role::CoolPropTests,
        },
        tables: &[Table { file: "paper/R1224YDZ/Akasaka-IJT-2023-R1224ydZ.7.csv", kind: TableKind::K2, rows: None }],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "HeavyWater",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "IAPWS-R16-17", doi_or_report: Some("IAPWS R16-17"), role: Role::Release },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "HeavyWater",
        part: ArbiterPart::Melting,
        citation: Citation {
            key: "Herrig-JPCRD-2019",
            doi_or_report: Some("10.1063/1.5053993"),
            role: Role::PropertyPaper,
        },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "Ammonia",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "Gao-JPCRD-2020", doi_or_report: Some("10.1063/5.0128269"), role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "CarbonDioxide",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "Span-JPCRD-1996", doi_or_report: Some("10.1063/1.555991"), role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "R125",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "Lemmon-JPCRD-2005", doi_or_report: Some("10.1063/1.1797813"), role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "Air",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "Lemmon-JPCRD-2000", doi_or_report: Some("10.1063/1.1285884"), role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "Nitrogen",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "Span-JPCRD-2000", doi_or_report: Some("10.1063/1.1349047"), role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "HFE143m",
        part: ArbiterPart::AlphaR,
        citation: Citation {
            key: "Akasaka-IJR-2012",
            doi_or_report: Some("10.1016/j.ijrefrig.2012.01.003"),
            role: Role::EosPaper,
        },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Expected,
    },
    Arbiter {
        fluid: "Propylene",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "Lemmon-PROPYLENE-2013", doi_or_report: None, role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Unpublished,
    },
    Arbiter {
        fluid: "SES36",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "SES36", doi_or_report: None, role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Unpublished,
    },
    Arbiter {
        fluid: "Neon",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "Thol-JPCRD-2019-Neon", doi_or_report: None, role: Role::EosPaper },
        tables: &[],
        constants: None,
        status: ArbiterStatus::Unpublished,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED: Arbiter = Arbiter {
        fluid: "Water",
        part: ArbiterPart::AlphaR,
        citation: Citation { key: "IAPWS-95", doi_or_report: Some("IAPWS R6-95(2018)"), role: Role::Release },
        tables: &[Table { file: "paper/Water/IAPWS-95.6.csv", kind: TableKind::K1, rows: Some(1) }],
        constants: None,
        status: ArbiterStatus::Expected,
    };

    fn rules(records: &[Arbiter], committed: &[&str], milestone: u8) -> Vec<String> {
        violations(records, &|file| committed.contains(&file), milestone)
    }

    /// VERIFICATION.md §4.3: the status follows the files, and only the evaluation (M4 on) may say a table arbitrates.
    #[test]
    fn statuses_follow_the_transcribed_files() {
        assert_eq!(rules(&[EXPECTED], &[], 2), Vec::<String>::new());
        let file = "paper/Water/IAPWS-95.6.csv";
        assert!(rules(&[EXPECTED], &[file], 2)[0].contains("Transcribed"), "a committed table is no longer Expected");
        let transcribed = Arbiter { status: ArbiterStatus::Transcribed, ..EXPECTED };
        assert_eq!(rules(&[transcribed], &[file], 2), Vec::<String>::new());
        assert!(!rules(&[transcribed], &[], 2).is_empty(), "Transcribed needs its files");
        let evaluated = Arbiter { status: ArbiterStatus::SelfConsistent, ..transcribed };
        assert!(!rules(&[evaluated], &[file], 4).is_empty(), "nothing is evaluated while M4 is open");
        assert_eq!(rules(&[evaluated], &[file], 5), Vec::<String>::new());
        let inconsistent = Arbiter { status: ArbiterStatus::Inconsistent { residual: 1.4e-6 }, ..transcribed };
        assert!(!rules(&[inconsistent], &[file], 3).is_empty());
        assert!(!rules(&[evaluated], &[], 5).is_empty(), "an evaluated table is committed");
    }

    #[test]
    fn records_cite_their_source_once() {
        let unpublished = Arbiter {
            fluid: "Neon",
            citation: Citation { key: "Thol-JPCRD-2019-Neon", doi_or_report: None, role: Role::EosPaper },
            tables: &[],
            status: ArbiterStatus::Unpublished,
            ..EXPECTED
        };
        assert_eq!(rules(&[EXPECTED, unpublished], &[], 2), Vec::<String>::new());
        let uncited = Arbiter { citation: Citation { doi_or_report: None, ..EXPECTED.citation }, ..EXPECTED };
        assert!(rules(&[uncited], &[], 2)[0].contains("DOI or report"));
        let vague =
            Arbiter { citation: Citation { doi_or_report: Some("the paper"), ..EXPECTED.citation }, ..EXPECTED };
        assert!(!rules(&[vague], &[], 2).is_empty());
        assert!(!rules(&[EXPECTED, EXPECTED], &[], 2).is_empty(), "one record per (fluid, part)");
        let listed = Arbiter { status: ArbiterStatus::None, ..EXPECTED };
        assert!(!rules(&[listed], &[], 2).is_empty(), "a stated absence lists no table");
        let none = Arbiter { status: ArbiterStatus::None, tables: &[], ..EXPECTED };
        assert_eq!(rules(&[none], &[], 2), Vec::<String>::new());
    }
}
