# PLAN.md section 6 decisions (2026-10-05)

| # | Question (PLAN.md §6) | Decision | Notes |
|---|---|---|---|
| P2 | CI host and remote | **GitHub + GitHub Actions**, repo `Dzoay/phasekit` | The agent may create the repo (`gh` authenticated, SSH set up). Commits use the GitHub identity only: `Dzoay <3277116+Dzoay@users.noreply.github.com>`, repo-local config; never a personal email. Rules for README / AGENTS.md: linear history through fast-forward PRs, Conventional Commits, SemVer. |
| P2b | Visibility | **Public from the start** | Licence and NOTICE files must be correct from the first push. |
| P3 | Performance reference machine | **The dev box** (i7-8700K, 6 cores / 12 threads, AVX2) | CPU model recorded with every result; CI tracks relative regressions only. |
| P4 | Paywalled EOS papers (Ammonia, CO₂, R125, Air, Nitrogen, HFE143m) | **Undecided: the user will look into cost and access later** | Until then these six stay oracle-only, labelled provisional; each upgrades to paper-verified when its paper arrives. No milestone waits on them. PDFs never enter the repo. |
| P1 | Shim library file name | **phasekit name only** | e.g. `phasekit_coolproplib.dll` / `libphasekit_coolproplib.so`; MIGRATION.md shows hosts how to point at it (or rename locally). Nothing is distributed under CoolProp's file names. |
| P6 | Shim `set_config_*` | **Translate where an equivalent exists** | The shim keeps its own process-wide settings (quirks live only in the shim) and turns keys with a phasekit equivalent into per-call options on every shim call (e.g. `DONT_CHECK_PROPERTY_LIMITS` → `DomainPolicy::Extrapolate`); other keys are accepted, ignored and listed in MIGRATION.md. No global state reaches the kernel. |
| P5 | Publishing v0.1 | **crates.io + npm at v0.1** | Under the `Dzoay` accounts via trusted publishing from GitHub Actions (no stored tokens); C library binaries as GitHub release assets; PyPI at M16. |
| P7 | How PRs land on `main` | **Squash merge** | One Conventional Commit per PR (the PR title), linear history; squash is the only enabled merge method. |
