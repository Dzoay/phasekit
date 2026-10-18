# A complexity refactor during M7 (2026-10-11)

Asked after a complexity audit of the workspace at `c094914` (#104; rust-code-analysis 0.0.25 for cognitive complexity
as Sonar specifies it, tree-sitter-rust 0.24.2 for nesting, generated.rs excluded): of 1 739 functions, 33 have a
cognitive complexity over 15 (Sonar's S3776), 33 nest control flow deeper than 3 (S134) and 5 run over 100 lines; 54
cross at least one bar. Tests nest deepest (22 of the 33: loops over fluids, points and pairs around a `match` that
collects failures, six deep in core's `input.rs`); core's `flash.rs` holds the library's five worst functions, which
hand-roll the same three search scaffolds. SonarQube itself is not adopted: for Rust it needs a server or an account,
runs only in CI, and its rules are not pinned with the repository. Clippy's lints of the same family gate instead.

| # | Question | Decision | Notes |
|---|---|---|---|
| CX1 | Where do the steps go? | **M7.9c-M7.9f, after M7.9b and before M7.10** | The gate first, so M7.10's new check routines and tests are held to the bar; then the fixes, each removing its exemptions. |
| CX2 | What bar? | **Sonar's: cognitive complexity 15, control-flow nesting 3, 100 lines per function** | 54 functions at `c094914`; outliers only (25 and 4) would have caught about 20. M7.9c calibrates clippy's thresholds to it; a permanent exemption (an algorithm as printed, a data table) states its reason. |
| CX3 | Production code and tools in one step or two? | **Two: M7.9e (core's flash scaffolds), M7.9f (verify's library and xtask)** | Core's diff needs bit-identical states as its proof; the others do not. |
| CX4 | How does M7.9c gate the bar? (asked after M7.9c's calibration run) | **An xtask gate on `syn` (`gates complexity`) for cognitive complexity and nesting as Sonar specifies them; clippy's `too_many_lines` at 100 for length** | Supersedes CX2's note on calibrating clippy. At `c094914` clippy's `too_many_lines` matches the length bar (4 functions over 100 code lines); `excessive_nesting` counts every block, not control flow (61 functions at 4, 32 of them not over S134's 3); the nursery `cognitive_complexity` is another measure (20 of the 33 functions over 15 score 10 or less, `sx` among them; a tolerance test Sonar scores 2 gets 32). `syn` 3 is already in Cargo.lock through serde's derive; xtask is tier T4. |
