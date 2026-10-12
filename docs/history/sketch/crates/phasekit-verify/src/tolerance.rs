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
    SatMp,
    Flash,
    TransportDirect,
    TransportEcs,
    Paper,
    Smoke,
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
    Some(Tolerance::Absolute(0.5 * phasekit_core::math::powi(10.0, exp - decimals)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_digits_give_half_ulp() {
        let Some(Tolerance::Absolute(t)) = from_printed("21.17909") else { panic!() };
        assert!((t - 5e-6).abs() < 1e-18);
        let Some(Tolerance::Absolute(t)) = from_printed("1.5e-3") else { panic!() };
        assert!((t - 5e-5).abs() < 1e-18);
        assert_eq!(from_printed("abc"), None);
    }
}
