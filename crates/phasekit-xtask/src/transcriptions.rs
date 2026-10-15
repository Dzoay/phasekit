//! Tables transcribed into the workspace from the pinned CoolProp checkout, checked against it number for number.

#[cfg(test)]
mod tests {
    use crate::repo::Repo;

    /// The numeric tokens of the block of `text` after `start` up to the first `end`, as written.
    fn numbers<'a>(text: &'a str, start: &str, end: &str) -> Vec<&'a str> {
        let from = text.find(start).map_or(text.len(), |i| i + start.len());
        let block = text.get(from..).unwrap_or_default();
        let block = block.get(..block.find(end).unwrap_or(block.len())).unwrap_or_default();
        block
            .split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '-' | '+')))
            .filter(|token| token.chars().any(|c| c.is_ascii_digit()))
            .collect()
    }

    /// PLAN.md M11.5: phasekit-cubic's superancillary tables are v8.0.0's `cubicsuperancillary.h` (Bell and Deiters
    /// 2021), every coefficient and piece bound as the header writes it, in order, for each of the six expansions.
    #[test]
    fn cubic_superancillary_is_coolprops() {
        let repo = Repo::locate();
        let header = repo.read("reference/CoolProp/include/CoolProp/superancillary/cubicsuperancillary.h").unwrap();
        let tables = repo.read("crates/phasekit-cubic/src/superancillary/tables.rs").unwrap();
        for name in ["SRK_p", "SRK_rhoL", "SRK_rhoV", "PR_p", "PR_rhoL", "PR_rhoV"] {
            let want = numbers(&header, &format!("const auto {name} ="), "};");
            let got = numbers(&tables, &format!("const {}: &[Piece] = &[", name.to_uppercase()), "];");
            assert!(want.len() > 100, "{name}: {} numbers in the header", want.len());
            assert_eq!(got, want, "{name}");
        }
    }
}
