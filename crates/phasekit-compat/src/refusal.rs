//! What a facade reads of a refusal beyond its message (PLAN.md M29.6; user decisions EM15-EM18): its remedies, the
//! names nearest an unknown one, and each refused point of a batch as a flat record of its code, rule, limit and up to
//! four numbers, which the C ABI and the browser package copy out.

use phasekit_core::{DomainError, Error, Limit, LoadError, Registry, Remedy, nearest_names};

use crate::CompatError;
use crate::keys::output_names;

impl CompatError {
    /// What the caller can do (user decision EM15): core's remedies for a kernel error; for an unknown key, the nearest
    /// key ([`nearest`]); for two keys that make no input pair, another pair; for any other refusal, none.
    pub const fn remedies(&self) -> &'static [Remedy] {
        match self {
            CompatError::Core(e) => e.remedies(),
            CompatError::UnknownOutput(_) | CompatError::UnknownInput(_) => &[Remedy::NearestName],
            CompatError::InvalidPair(..) => &[Remedy::OtherPair],
            _ => &[],
        }
    }
}

/// Up to `k` names nearest the one a refusal did not know: a fluid name `registry` does not hold (its names, aliases and
/// CAS numbers, [`Registry::nearest`]), or an output key (compat's output keys); none for any other refusal.
pub fn nearest(registry: &Registry, error: &CompatError, k: usize) -> Vec<String> {
    let names: Vec<&str> = match error {
        CompatError::Core(Error::Load(LoadError::UnknownName(name))) => registry.nearest(name, k),
        CompatError::UnknownOutput(key) => nearest_names(key, output_names().map(|n| (n, n)), k),
        _ => Vec::new(),
    };
    names.into_iter().map(String::from).collect()
}

/// A refused point as a flat record (user decision EM17): its code, an invalid input's rule, the end of the range an
/// unreachable or out-of-domain value lies beyond, and up to four numbers, SI, NaN in the slots it does not use:
/// an invalid input's value; an unreachable value, its bound and the given input's value; a domain error's value and
/// its limit (and the melting temperature's pressure); an ambiguous input's roots; a stalled solve's last T, ρ and
/// residual.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detail {
    /// The refusal's code (`Error::code`).
    pub code: &'static str,
    /// An invalid input's rule's code, else "".
    pub rule: &'static str,
    /// The end of the range the value lies beyond, where it is one.
    pub limit: Option<Limit>,
    /// Its numbers.
    pub values: [f64; 4],
}

/// `error` as a [`Detail`].
pub fn detail(error: &Error) -> Detail {
    let (limit, numbers) = numbers(error);
    let mut values = [f64::NAN; 4];
    for (slot, number) in values.iter_mut().zip(numbers) {
        *slot = number;
    }
    Detail { code: error.code(), rule: error.rule().map_or("", |r| r.code()), limit, values }
}

/// The limit and the numbers of [`detail`].
fn numbers(error: &Error) -> (Option<Limit>, Vec<f64>) {
    match error {
        Error::InvalidInput { value, .. } => (None, vec![*value]),
        Error::Unreachable { value, limit, bound, given, .. } => (Some(*limit), vec![*value, *bound, given.1]),
        Error::Domain(d) => domain_numbers(d),
        Error::Ambiguous { roots } => (None, roots.as_slice().to_vec()),
        Error::NoConvergence { last: Some(last), .. } => (None, vec![last.t, last.rho, last.residual]),
        _ => (None, Vec::new()),
    }
}

fn domain_numbers(error: &DomainError) -> (Option<Limit>, Vec<f64>) {
    match *error {
        DomainError::BelowMinTemperature { t, t_min } => (Some(Limit::Min), vec![t, t_min]),
        DomainError::AboveMaxTemperature { t, t_max } => (Some(Limit::Max), vec![t, t_max]),
        DomainError::BelowMinPressure { p, p_min } => (Some(Limit::Min), vec![p, p_min]),
        DomainError::AboveMaxPressure { p, p_max } => (Some(Limit::Max), vec![p, p_max]),
        DomainError::BelowMeltingLine { t, t_melt, p } => (Some(Limit::Min), vec![t, t_melt, p]),
        _ => (None, Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "embedded")]
    use phasekit_core::DataSet;
    #[cfg(feature = "embedded")]
    use phasekit_core::batch::{ExecPolicy, Status};
    use phasekit_core::{Quantity, Roots, Strategy};

    use super::*;
    #[cfg(feature = "embedded")]
    use crate::{batch_si_detailed, fluid_in};

    /// PLAN.md M29.6 (user decision EM15): a refusal's remedies, core's for a kernel error and compat's own for a key.
    #[test]
    fn every_refusal_has_its_remedies() {
        let cases: [(CompatError, &[&str]); 6] = [
            (CompatError::UnknownOutput("Y".into()), &["nearest_name"]),
            (CompatError::UnknownInput("Y".into()), &["nearest_name"]),
            (CompatError::InvalidPair("T".into(), "T".into()), &["other_pair"]),
            (CompatError::InvalidFluid("A&B".into()), &[]),
            (CompatError::NotAConstant("Hmass".into()), &[]),
            (CompatError::Core(Error::Load(LoadError::UnknownName("Watr".into()))), &["nearest_name"]),
        ];
        for (error, codes) in cases {
            let got: Vec<_> = error.remedies().iter().map(|r| r.code()).collect();
            assert_eq!(got, codes, "{error:?}");
        }
    }

    /// The names nearest an unknown fluid (the registry's) and an unknown output key (compat's); none for any other
    /// refusal.
    #[test]
    #[cfg(feature = "embedded")]
    fn nearest_fluids_and_outputs() {
        let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
        let unknown = fluid_in(&registry, "Watr").unwrap_err();
        assert_eq!(nearest(&registry, &unknown, 1), ["Water"]);
        assert_eq!(nearest(&registry, &CompatError::UnknownOutput("Hmas".into()), 1), ["Hmass"]);
        assert!(nearest(&registry, &CompatError::InvalidFluid("A&B".into()), 3).is_empty());
    }

    /// User decision EM17: a refusal's flat record, each kind's numbers in their slots and NaN in the rest.
    #[test]
    fn a_detail_per_refusal() {
        let given = (Quantity::TEMPERATURE, 300.0);
        let unreachable =
            Error::Unreachable { quantity: Quantity::TEMPERATURE, value: 1e7, limit: Limit::Max, bound: 2e6, given };
        let cases = [
            (Error::InvalidInput { quantity: Quantity::PRESSURE, value: -1.0 }, "not_positive", None, vec![-1.0]),
            (unreachable, "", Some(Limit::Max), vec![1e7, 2e6, 300.0]),
            (
                DomainError::BelowMeltingLine { t: 250.0, t_melt: 251.0, p: 2e8 }.into(),
                "",
                Some(Limit::Min),
                vec![250.0, 251.0, 2e8],
            ),
            (
                Error::Ambiguous { roots: Roots::new(Quantity::TEMPERATURE, &[300.0, 350.0]) },
                "",
                None,
                vec![300.0, 350.0],
            ),
            (Error::stalled(Strategy::DensityNewton, 3, (300.0, 5e4), 1e-3), "", None, vec![300.0, 5e4, 1e-3]),
            (Error::no_convergence(Strategy::Vle, 3), "", None, vec![]),
        ];
        for (error, rule, limit, numbers) in cases {
            let d = detail(&error);
            let used: Vec<f64> = d.values.iter().copied().filter(|v| !v.is_nan()).collect();
            assert_eq!((d.code, d.rule, d.limit, used), (error.code(), rule, limit, numbers), "{error:?}");
        }
    }

    /// `batch_si_detailed` keeps each point's error as core's detailed batch does; without slots it is `batch_si_with`.
    #[test]
    #[cfg(feature = "embedded")]
    fn a_detailed_batch_keeps_each_error() {
        let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
        let water = fluid_in(&registry, "Water").unwrap();
        let (t, p) = ([300.0, -1.0, 200.0], [1e5, 1e5, 1e5]);
        let (mut out, mut status) = ([0.0; 3], [Status::Ok; 3]);
        let mut errors = [None, None, None];
        let inputs = [("T", t.as_slice()), ("P", p.as_slice())];
        batch_si_detailed(&water, ExecPolicy::Reference, &["Hmass"], inputs, (&mut out, &mut status), &mut errors)
            .unwrap();
        let codes: Vec<_> = errors.iter().map(|e| e.as_ref().map(Error::code)).collect();
        assert_eq!(codes, [None, Some("invalid_input"), Some("below_min_temperature")]);
    }
}
