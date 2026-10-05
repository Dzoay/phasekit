//! `gates doc-excerpts`: Rust code in the design documents is real code (ROT-139; map 01 R19). Every fenced `rust`
//! block in `docs/*.md` is split at blank lines into chunks, and each chunk must appear in one source file under
//! `crates/` or `docs/design/sketch/` with its lines in order, compared modulo indentation and `///`/`//!` lines;
//! an excerpt may leave lines out. A block preceded by `<!-- excerpt: illustrative -->` is not checked.

use super::{Verdict, no_args};
use crate::repo::Repo;

const ILLUSTRATIVE: &str = "<!-- excerpt: illustrative -->";

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let docs = repo.files("docs", ".md", false).map_err(|e| vec![e])?;
    let mut sources = repo.files("crates", ".rs", true).map_err(|e| vec![e])?;
    sources.extend(repo.files("docs/design/sketch", ".rs", true).map_err(|e| vec![e])?);
    if docs.is_empty() || sources.is_empty() {
        return Err(vec!["no documents or no sources to compare".to_string()]);
    }
    let checked = check(&docs, &sources)?;
    Ok(format!("{checked} rust blocks in {} documents match {} sources", docs.len(), sources.len()))
}

/// Checks the (path, text) documents against the (path, text) sources; returns how many blocks were checked.
fn check(docs: &[(String, String)], sources: &[(String, String)]) -> Result<usize, Vec<String>> {
    let sources: Vec<Vec<&str>> = sources.iter().map(|(_, text)| normalized(text.lines())).collect();
    let (mut checked, mut errors) = (0, Vec::new());
    for (path, text) in docs {
        let lines: Vec<&str> = text.lines().collect();
        let mut open = 0;
        while let Some(start) = (open..lines.len()).find(|&i| lines[i].trim().starts_with("```rust")) {
            let Some(close) = (start + 1..lines.len()).find(|&i| lines[i].trim().starts_with("```")) else {
                errors.push(format!("{path}:{}: unterminated rust block", start + 1));
                break;
            };
            open = close + 1;
            if lines[..start].iter().rev().find(|line| !line.trim().is_empty()).map(|line| line.trim())
                == Some(ILLUSTRATIVE)
            {
                continue;
            }
            checked += 1;
            // Chunks are separated by blank lines; each must appear in order in one source.
            let mut chunk_start = start + 1;
            for i in start + 1..=close {
                if i < close && !lines[i].trim().is_empty() {
                    continue;
                }
                let chunk = normalized(lines[chunk_start..i].iter().copied());
                if !chunk.is_empty() && !sources.iter().any(|source| in_order(source, &chunk)) {
                    errors.push(format!(
                        "{path}:{}: the excerpt starting `{}` is not in any source under crates/ or docs/design/sketch/ \
                         (lines in order, modulo indentation and doc comments); fix it or mark it {ILLUSTRATIVE}",
                        chunk_start + 1,
                        chunk[0]
                    ));
                }
                chunk_start = i + 1;
            }
        }
    }
    if errors.is_empty() { Ok(checked) } else { Err(errors) }
}

/// Lines compared modulo indentation, without blank lines and `///`/`//!` doc comments.
fn normalized<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    lines
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("///") && !line.starts_with("//!"))
        .collect()
}

/// Whether every chunk line appears in `source`, in the same order (lines in between may be elided).
fn in_order(source: &[&str], chunk: &[&str]) -> bool {
    let mut rest = source.iter();
    chunk.iter().all(|line| rest.any(|candidate| candidate == line))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "impl Fluid {
    /// Wraps a model at its native gauge.
    pub fn new(model: Arc<dyn ThermoModel>) -> Fluid {
        Fluid { model, gauge: Gauge::NATIVE }
    }

    pub fn gauge(&self) -> Gauge {
        self.gauge
    }
}
";
    const OTHER: &str = "pub fn bundle_from_gibbs(r: f64) -> f64 {\n    r\n}\n";

    fn doc(marker: &str, block: &str) -> Vec<(String, String)> {
        vec![("docs/ARCHITECTURE.md".to_string(), format!("Text.\n{marker}\n```rust\n{block}```\n"))]
    }

    /// Rot: ROT-139. Map 01 R19: design documents drift from the code.
    #[test]
    fn a_doc_excerpt_not_in_the_sources_is_rejected() {
        let sources = [("crates/phasekit-core/src/fluid.rs", SOURCE), ("crates/phasekit-core/src/state.rs", OTHER)]
            .map(|(path, text)| (path.to_string(), text.to_string()));
        // Verbatim modulo indentation and `///` lines, eliding the lines between the two functions; a second chunk
        // (after a blank line) may come from another file.
        let excerpt = "pub fn new(model: Arc<dyn ThermoModel>) -> Fluid {\n    Fluid { model, gauge: Gauge::NATIVE }\n}\n\
                       pub fn gauge(&self) -> Gauge {\n\npub fn bundle_from_gibbs(r: f64) -> f64 {\n";
        assert_eq!(check(&doc("", excerpt), &sources), Ok(1));
        // A drifted line, or the right lines in the wrong order, is rejected with the chunk's location.
        let errors = check(&doc("", &excerpt.replace("Gauge::NATIVE", "Gauge::default()")), &sources).unwrap_err();
        assert!(errors.len() == 1 && errors[0].starts_with("docs/ARCHITECTURE.md:4:"), "{errors:?}");
        let swapped = "pub fn gauge(&self) -> Gauge {\npub fn new(model: Arc<dyn ThermoModel>) -> Fluid {\n";
        assert!(check(&doc("", swapped), &sources).is_err());
        // An unterminated block is an error; a block marked illustrative is not checked.
        assert!(check(&[("docs/A.md".to_string(), "```rust\nfn f() {}\n".to_string())], &sources).is_err());
        assert_eq!(check(&doc(ILLUSTRATIVE, swapped), &sources), Ok(0));
    }
}
