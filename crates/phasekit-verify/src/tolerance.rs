//! Where expected values come from, and how close is close enough (map 10 §8.3, map 13).

/// Provenance of an expected value. Only the first three can arbitrate against CoolProp.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Provenance {
    /// A printed check table: citation, table, row.
    Paper {
        /// BibTeX key.
        citation: &'static str,
        /// Table label.
        table: &'static str,
    },
    /// An IAPWS release table.
    Iapws {
        /// Release, e.g. "R6-95(2018)".
        release: &'static str,
    },
    /// Multiprecision reference points (e.g. 390 saturation points, map 10 §8.1).
    MultiPrecision {
        /// Source.
        source: &'static str,
    },
    /// The CoolProp oracle (provisional).
    Oracle {
        /// Version, e.g. "8.0.0+ae81610e".
        version: &'static str,
    },
    /// Another implementation (teqp, Clapeyron): differential, never an arbiter.
    OtherImplementation {
        /// Name and version.
        name: &'static str,
    },
    /// Values computed by the code under test (map 10 R6): never an arbiter.
    SelfReferential,
    /// A thermodynamic identity.
    Identity,
}

impl Provenance {
    /// Can this value overrule the oracle?
    pub fn is_arbiter(&self) -> bool {
        matches!(self, Provenance::Paper { .. } | Provenance::Iapws { .. } | Provenance::MultiPrecision { .. })
    }
}

/// The floor of class `Term` (VERIFICATION.md §5): the smallest Σ_k |φ_k| an entry is compared against.
pub const TERM_FLOOR: f64 = 1e-300;

/// Where an entry lies, for the classes whose bound differs in the near-critical window "nc" (VERIFICATION.md §5):
/// |T/Tc − 1| < 1e-3 and |ρ/ρc − 1| < 0.1 at the published Tc and ρc (map 10 §8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// Outside the near-critical window.
    Regular,
    /// Inside it.
    NearCritical,
}

impl Window {
    /// The window of (T, ρ) for a fluid whose published critical point is (Tc, ρc).
    pub fn at(t: f64, rho: f64, tc: f64, rhoc: f64) -> Window {
        if (t / tc - 1.0).abs() < 1e-3 && (rho / rhoc - 1.0).abs() < 0.1 {
            Window::NearCritical
        } else {
            Window::Regular
        }
    }
}

/// An absolute or relative bound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tolerance {
    /// |a − b| ≤ x.
    Absolute(f64),
    /// |a − b| ≤ x·|b|.
    Relative(f64),
}

/// The named classes of map 10 §8.3. Never widened to make CoolProp pass (map 10 R4/R5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(missing_docs)] // values are defined in docs/VERIFICATION.md
pub enum ToleranceClass {
    Exact,
    Term,
    Prop,
    SaCoeff,
    SaFit,
    CaloricFit,
    SatMp,
    Flash,
    TransportDirect,
    TransportEcs,
    Paper,
    Measured,
    Identity,
    Fd,
    RefAnchor,
    Smoke,
}

impl ToleranceClass {
    /// Every class with its name in fixture `tol:` lines (VERIFICATION.md §3.3, §5).
    const NAMES: [(ToleranceClass, &'static str); 16] = [
        (ToleranceClass::Exact, "exact"),
        (ToleranceClass::Term, "term"),
        (ToleranceClass::Prop, "prop"),
        (ToleranceClass::SaCoeff, "sa_coeff"),
        (ToleranceClass::SaFit, "sa_fit"),
        (ToleranceClass::CaloricFit, "caloric_fit"),
        (ToleranceClass::SatMp, "sat_mp"),
        (ToleranceClass::Flash, "flash"),
        (ToleranceClass::TransportDirect, "transport_direct"),
        (ToleranceClass::TransportEcs, "transport_ecs"),
        (ToleranceClass::Paper, "paper"),
        (ToleranceClass::Measured, "measured"),
        (ToleranceClass::Identity, "identity"),
        (ToleranceClass::Fd, "fd"),
        (ToleranceClass::RefAnchor, "ref_anchor"),
        (ToleranceClass::Smoke, "smoke"),
    ];

    /// The class's name in fixture `tol:` lines.
    pub fn name(self) -> &'static str {
        Self::NAMES.iter().find(|(class, _)| *class == self).map_or("", |(_, name)| name)
    }

    /// The class a `tol:` name stands for.
    pub fn from_name(name: &str) -> Option<ToleranceClass> {
        Self::NAMES.iter().find(|(_, n)| *n == name).map(|(class, _)| *class)
    }

    /// The absolute bound for an entry of magnitude `scale` outside the near-critical window: [`Self::bound_in`].
    pub fn bound(self, scale: f64) -> Option<f64> {
        self.bound_in(scale, Window::Regular)
    }

    /// The absolute bound for an entry of magnitude `scale` in `window` (VERIFICATION.md §5), for the classes compared
    /// numerically so far; `None` for the others until the first fixture kind compared under each lands (PLAN.md M3
    /// on). `Term` takes Σ_k |φ_k| from [`crate::majorant`] and applies its floor; `Prop` takes max(|value|, floor)
    /// with the floors of §5 (the caller's, since they depend on the quantity) and `Identity` the largest term.
    pub fn bound_in(self, scale: f64, window: Window) -> Option<f64> {
        let near = window == Window::NearCritical;
        match self {
            ToleranceClass::Term => Some(1e-13 * scale.max(TERM_FLOOR)),
            ToleranceClass::Prop | ToleranceClass::Identity => Some(if near { 1e-8 } else { 1e-12 } * scale),
            ToleranceClass::SaCoeff => Some(1e-14 * scale),
            ToleranceClass::CaloricFit => Some(2e-6 * scale),
            // T, ρ, p, h, s, u; the near-critical values for ρ and T apply to solved inputs (M7), not to DT's.
            ToleranceClass::Flash => Some(1e-9 * scale),
            _ => None,
        }
    }

    /// The bound of an output a relation computes from the α derivatives (`Prop`, VERIFICATION.md §5): the class's own
    /// bound on `scale` in `window`, or `Term`'s bound on `carried` = Σ_ij |∂X/∂A_ij|·M_ij (the entries' `Term` scales
    /// carried through the relation to first order) where that is larger. A NaN `carried` leaves the own bound.
    pub fn bound_carried(self, scale: f64, window: Window, carried: f64) -> Option<f64> {
        Some(self.bound_in(scale, window)?.max(ToleranceClass::Term.bound(carried)?))
    }

    /// `Flash`'s bound on a vapour quality (VERIFICATION.md §5): 1e-8, absolute.
    pub const FLASH_QUALITY: f64 = 1e-8;

    /// `SaFit`'s bound at one multiprecision point (VERIFICATION.md §5): 4 · |SA/mp − 1| of that point, floor 1e-14,
    /// relative to the multiprecision `value` (CoolProp's own acceptance rule, map 10 §3).
    pub fn sa_fit(ratio: f64, value: f64) -> f64 {
        (4.0 * (ratio - 1.0).abs()).max(1e-14) * value.abs()
    }

    /// Every class with its bound as VERIFICATION.md §5 states it, in that table's order.
    pub fn table() -> Vec<(ToleranceClass, &'static str)> {
        Self::BOUNDS.to_vec()
    }

    /// VERIFICATION.md §5, the "Bound" column verbatim: a class changes here and there together.
    const BOUNDS: [(ToleranceClass, &'static str); 16] = [
        (ToleranceClass::Exact, "bitwise (`to_bits`), statuses equal"),
        (ToleranceClass::Term, "1e-13 · Σ_k abs(φ_k), floor 1e-300"),
        (ToleranceClass::Prop, "1e-12; nc 1e-8; or `Term` carried through the relation, if larger"),
        (ToleranceClass::SaCoeff, "1e-14"),
        (ToleranceClass::SaFit, "4 · abs(SA/mp − 1) of that point, floor 1e-14"),
        (ToleranceClass::CaloricFit, "2e-6 of max(abs(value), floor)"),
        (ToleranceClass::SatMp, "p, ρ 1e-11; ρ 1e-6 if Θ < 1e-3"),
        (ToleranceClass::Flash, "T, ρ, p, h, s, u 1e-9; Q 1e-8 abs; nc: ρ 1e-6, T 1e-8"),
        (ToleranceClass::TransportDirect, "1e-12"),
        (ToleranceClass::TransportEcs, "1e-8"),
        (ToleranceClass::Paper, "half a unit in the last printed digit (`from_printed`)"),
        (ToleranceClass::Measured, "the residual recorded in a register entry, rounded up to one significant digit"),
        (ToleranceClass::Identity, "1e-12 of the largest term of the identity; nc 1e-8"),
        (ToleranceClass::Fd, "1e-7, central differences, relative step 1e-5"),
        (ToleranceClass::RefAnchor, "1e-8 abs, SI mass units *(inference: map 01 §8 states 1e-8 without units)*"),
        (ToleranceClass::Smoke, "finite value or a documented error class"),
    ];
}

/// Half a unit in the last printed digit: the tolerance a printed check value supports.
/// `"21.17909"` → `Absolute(5e-6)`; exponents are honoured (`"1.5e-3"` → `Absolute(5e-5)`).
pub fn from_printed(printed: &str) -> Option<Tolerance> {
    let (mantissa, exp) = match printed.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (printed, 0),
    };
    mantissa.parse::<f64>().ok()?;
    let decimals = mantissa.split_once('.').map_or(0, |(_, frac)| frac.len());
    let decimals = i32::try_from(decimals).ok()?;
    // "5e<n>" parses to the double nearest half a unit; 0.5·10^n through powi can land an ulp away.
    format!("5e{}", exp - decimals - 1).parse().ok().map(Tolerance::Absolute)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERIFICATION: &str = include_str!("../../../docs/VERIFICATION.md");

    /// The (class, bound) rows of VERIFICATION.md §5's table.
    fn documented() -> Vec<(String, String)> {
        let section = VERIFICATION.split("## 5. Tolerance classes").nth(1).unwrap_or_default();
        let section = section.split("\n## 6.").next().unwrap_or_default();
        section
            .lines()
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|row| {
                let (class, rest) = row.split_once("` | ")?;
                Some((class.to_string(), rest.split(" | ").next()?.to_string()))
            })
            .collect()
    }

    /// The class table in code equals VERIFICATION.md §5 row for row, so a class changes in both or neither, with
    /// a derivation (map 10 R4/R5: never widened to make CoolProp pass). ROT-130.
    #[test]
    fn tolerance_classes_match_verification_md() {
        let code: Vec<(String, String)> = ToleranceClass::table()
            .into_iter()
            .map(|(class, bound)| (format!("{class:?}"), bound.to_string()))
            .collect();
        assert_eq!(code, documented());
        assert_eq!(code.len(), 16);
        for (class, _) in ToleranceClass::table() {
            assert_eq!(ToleranceClass::from_name(class.name()), Some(class), "{class:?} has a tol: name");
        }
        assert_eq!(ToleranceClass::from_name("ref_anchor"), Some(ToleranceClass::RefAnchor));
    }

    /// Map 10 §8.4, DIV-0014: R1224YDZ's printed 21.17909 MPa supports half a unit, 5e-6 MPa; the oracle's
    /// 21.1790735 MPa lies 3.3 half-units away, so the printed row rejects the oracle.
    #[test]
    fn r1224ydz_printed_p_rejects_the_oracle() {
        let Some(Tolerance::Absolute(half)) = from_printed("21.17909") else {
            panic!("a printed value has a tolerance")
        };
        assert_eq!(half.to_bits(), 5e-6_f64.to_bits());
        let units = (21.1790735_f64 - 21.17909).abs() / half;
        assert!(units > 1.0, "the oracle lies outside the printed digits");
        assert_eq!(format!("{units:.1}"), "3.3");
    }

    /// Map 10 R6: values computed by the code under test (R-self) or by another implementation (R-other) never
    /// arbitrate; printed tables, IAPWS releases and multiprecision points do. ROT-131.
    #[test]
    fn self_referential_rows_are_not_arbiters() {
        assert!(!Provenance::SelfReferential.is_arbiter());
        assert!(!Provenance::OtherImplementation { name: "teqp 0.23" }.is_arbiter());
        assert!(!Provenance::Oracle { version: "8.0.0" }.is_arbiter() && !Provenance::Identity.is_arbiter());
        assert!(Provenance::Paper { citation: "lemmon2016", table: "7" }.is_arbiter());
        assert!(Provenance::Iapws { release: "R6-95(2018)" }.is_arbiter());
        assert!(Provenance::MultiPrecision { source: "coolprop-json" }.is_arbiter());
    }

    /// `Prop` and `Identity` bound 1e-12 of their scale, 1e-8 in the near-critical window, whose edges are strict (the
    /// window of map 10 §8.3: |T/Tc − 1| < 1e-3 and |ρ/ρc − 1| < 0.1); `Prop` also takes `Term` on a carried scale.
    #[test]
    fn prop_and_identity_relax_only_near_critical() {
        use ToleranceClass::{Identity, Prop, Term};
        for class in [Prop, Identity] {
            assert_eq!(class.bound(2.0), Some(2e-12));
            assert_eq!(class.bound_in(2.0, Window::NearCritical), Some(2e-8));
        }
        assert_eq!(Term.bound_in(1.0, Window::NearCritical), Term.bound(1.0));
        // `Prop` keeps its own bound unless `Term` on the carried scale is larger; a NaN carried scale changes nothing.
        assert_eq!(Prop.bound_carried(2.0, Window::Regular, 10.0), Some(2e-12));
        assert_eq!(Prop.bound_carried(2.0, Window::Regular, 30.0), Some(3e-12));
        assert_eq!(Prop.bound_carried(2.0, Window::NearCritical, 1e5), Some(2e-8));
        assert_eq!(Prop.bound_carried(2.0, Window::Regular, f64::NAN), Some(2e-12));
        assert_eq!(ToleranceClass::SatMp.bound_carried(2.0, Window::Regular, 1.0), None);
        // `SaCoeff` is 1e-14 relative; `SaFit` 4 times the point's own SA/mp misfit, never below 1e-14.
        assert_eq!(ToleranceClass::SaCoeff.bound_in(3.0, Window::NearCritical), Some(3e-14));
        assert_eq!(ToleranceClass::CaloricFit.bound(4.0), Some(8e-6));
        assert_eq!(ToleranceClass::Flash.bound_in(2.0, Window::NearCritical), Some(2e-9));
        let misfit = 1.0 / 1_073_741_824.0; // SA/mp − 1 = 2^-30, exact
        assert_eq!(ToleranceClass::sa_fit(1.0 + misfit, -2.0), 8.0 * misfit);
        assert_eq!(ToleranceClass::sa_fit(1.0 - misfit, 2.0), 8.0 * misfit);
        assert_eq!((ToleranceClass::sa_fit(1.0, 3.0), ToleranceClass::sa_fit(1.0 + 1e-16, 1.0)), (3e-14, 1e-14));
        assert_eq!(Term.bound(0.0), Some(1e-313));
        let (tc, rhoc) = (600.0, 10_000.0);
        assert_eq!(Window::at(600.5, 10_900.0, tc, rhoc), Window::NearCritical);
        assert_eq!(Window::at(599.5, 9_100.0, tc, rhoc), Window::NearCritical);
        for (t, rho) in [(600.6, 10_000.0), (599.4, 10_000.0), (600.0, 11_000.0), (600.0, 8_990.0)] {
            assert_eq!(Window::at(t, rho, tc, rhoc), Window::Regular, "{t} {rho}");
        }
    }

    #[test]
    fn printed_digits_give_half_ulp() {
        let Some(Tolerance::Absolute(t)) = from_printed("21.17909") else { panic!() };
        assert!((t - 5e-6).abs() < 1e-18);
        let Some(Tolerance::Absolute(t)) = from_printed("1.5e-3") else { panic!() };
        assert!((t - 5e-5).abs() < 1e-18);
        assert_eq!(from_printed("abc"), None);
    }
}
