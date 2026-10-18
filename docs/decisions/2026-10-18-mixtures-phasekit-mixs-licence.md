# Mixtures: phasekit-mix's licence (2026-10-18)

Asked at M13.14 and again at M13's close: phasekit-mix transcribes GERG-2008's coefficients from NIST's AGA8 code
(MX7), whose notice `LICENSE-THIRD-PARTY` and `LICENSES/LicenseRef-NIST-GERG.txt` carry, while the crate's Cargo
`license` was the workspace's alone.

| # | Question | Decision | Notes |
|---|---|---|---|
| MX10 | phasekit-mix's Cargo `license` | **`(MIT OR Apache-2.0) AND LicenseRef-NIST-GERG`, before it is ever published** ("yes, phasekit-mix's Cargo license field should carry LicenseRef-NIST-GERG before it's ever published") | The crate ships the notice as `crates/phasekit-mix/LICENSE-NIST-GERG`, `LICENSES/LicenseRef-NIST-GERG.txt` byte for byte; `deny.toml` allows the `LicenseRef` for phasekit-mix alone and `about.toml` accepts it there. crates.io parses `license` with unknown identifiers refused, and the SPDX parser it uses admits `LicenseRef-` under that mode, so the expression should publish; the first publish confirms it. Publishing stays the user's call. |
