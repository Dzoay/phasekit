# Notices for the lane ports (2026-10-17)

Found while planning in-house maths: M12's lane `exp`, `ln` and `expm1` (`crates/phasekit-core/src/num/lanes/`) port
the `libm` crate 0.2.16's operation for operation, constants included. That crate is MIT, and its sources are FreeBSD's
msun under Sun Microsystems' notices ("provided that this notice is preserved"). Neither notice was carried:
`REUSE.toml` annotated the three files as the project's own, and `LICENSE-THIRD-PARTY` did not name them.

| # | Question | Decision | Notes |
|---|---|---|---|
| LC1 | Credit the `libm` crate and Sun for the lane ports? | **Yes, now, in a pull request of its own** | `REUSE.toml` annotates the three files `(MIT OR Apache-2.0) AND MIT AND SunPro` (`LICENSES/SunPro.txt`), and `LICENSE-THIRD-PARTY`, which the C library, the npm package and the site ship, reproduces the crate's MIT notice and the three msun notices. The core is frozen until the `m13` tag (MX2), so the files' own heads (each its msun file's notice) and a `crates/phasekit-core/NOTICE` in the crate's package come after it, before any crates.io publish. |
