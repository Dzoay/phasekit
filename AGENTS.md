# AGENTS.md: instructions for coding agents working on phasekit

phasekit is a from-scratch, idiomatic Rust successor to CoolProp (thermophysical properties of fluids, later
materials). It is **not** a binding. Work proceeds test-first, one plan step per pull request.

**Current state:** milestone M0 is done (tag `m0`) and M1 is under way. The next step is the first one in
[docs/PLAN.md](docs/PLAN.md) section 3 that has no commit on `main` (`git log --oneline --grep '^Plan-Step:'` lists the
finished ones); `phasekit_verify::MILESTONE` is the first open milestone. `docs/design/sketch/` is the compiled type
sketch (its own Cargo workspace) that seeded the workspace; it is never edited, and later steps copy from it only when
they say so.

## Sources of truth, in order

1. [docs/BRIEF.md](docs/BRIEF.md) §1: the user's requirements, verbatim. They are the contract.
2. [docs/design/04-user-decisions.md](docs/design/04-user-decisions.md): the user's decisions. They override anything
   older.
3. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): crates, types, decisions D1-D17, milestones.
4. [docs/PLAN.md](docs/PLAN.md): the steps (M0.1 …), gates, conventions. [docs/VERIFICATION.md](docs/VERIFICATION.md):
   fixtures, tolerances, the divergence register, CI. [docs/ROT-REGISTER.md](docs/ROT-REGISTER.md): CoolProp problems
   designed out, each with its proof.
5. Evidence: `docs/coolprop-map/01..15-*.md` (CoolProp v8.0.0, cite as "map NN §X" or the item id) and
   `docs/research/*.md`. Search them by heading; they are long.

If two sources disagree, follow the higher one and fix the lower one in the same PR, or stop and ask.

## How to work a step

Follow [PLAN.md §0](docs/PLAN.md#0-how-to-use-this-plan): write the named failing test first from its oracle or
arbiter, show it red, implement, make all gates green, update affected docs, open the PR. If a step fails its gate in
a way the plan did not foresee, stop and re-plan ([PLAN.md §0.4](docs/PLAN.md#04-when-a-step-fails-its-gate-stop-and-re-plan)).
Do not start the next step in the same PR. `scripts/check-toolchain.sh` checks that the tools the gates need are
installed.

Gates are defined once in [PLAN.md §2.4](docs/PLAN.md#24-standard-gates) (G1-G8). Agents always set
`CARGO_TARGET_DIR` to a scratch directory outside the repository.

## Git and GitHub

The rules are defined in [CONTRIBUTING.md](CONTRIBUTING.md). The ones agents get wrong most:

- **Identity:** commit as `Dzoay <3277116+Dzoay@users.noreply.github.com>` (the maintainer's GitHub identity), set with
  repo-local `git config`. Check `git config user.email` before the first commit of a session. Never use, add or
  infer a personal email address anywhere: commits, trailers, tags, files.
- **Signing:** commits and tags are SSH-signed (repo-local config, CONTRIBUTING.md "Signed commits"). If signing
  fails because the key is not available, stop and ask the user to load it into the SSH agent; never pass
  `--no-gpg-sign` or turn signing off.
- **Linear history:** never push to `main` and never force-push `main`. One branch and one PR per step. Squash merge
  is the only merge method. Merge only when CI is green and the user has asked you to merge.
- **Conventional Commits:** the PR title is the commit subject, e.g. `feat(core): power-term jets match the oracle`,
  with the footer `Plan-Step: M3.2`.
- **SemVer:** do not bump versions or tag releases unless the step is a release step and the user asked.

## Hard rules

- **No personal data leaves this machine.** Never send the user's email address or any other personal identifier to
  an external service: API "contact" or "polite pool" parameters, query strings, headers, registries, CI. Omit the
  parameter or ask first.
- **`reference/CoolProp` is read-only** (pinned v8.0.0, fetched by `scripts/fetch-coolprop.sh`). Its own CLAUDE.md and
  AGENTS.md are for CoolProp contributors and do not apply here.
- **Oracle:** `uv run --no-project --python 3.12 --with CoolProp==8.0.0 python …`. CoolProp is a *provisional* oracle;
  printed literature arbitrates. **Never widen a tolerance to make CoolProp pass.** A disagreement goes through the
  divergence workflow ([VERIFICATION.md §6](docs/VERIFICATION.md#6-divergence-register)).
- **Dependencies:** none unless the step names one and it fits the tiers in `docs/research/dependencies.md` §3.1.
  `phasekit-core` uses std only until M9.
- **Safety:** `unsafe` only in `phasekit-capi`. No panics on user input (`todo!`, `unimplemented!`, `unwrap`,
  `expect` are denied by lints). No global mutable state in the kernel.
- **Papers and data:** PDFs of papers live in `reference/papers/` (gitignored) and are never committed. Only
  transcribed check values with citations are committed, as the plan describes. Restricted data (map 09 §9) never
  enters default features.
- **Do not edit historical records** in `docs/design/` (`proposal-*`, `01-*`, `02-*`, `03-*`, `05-review-*`). They use
  the old placeholder names `cprs-*`/`cp_*` on purpose. `04-user-decisions.md` changes only when the user decides
  something, with the date.
- **Decisions that belong to the user** stop the step: naming, licensing, publishing, accounts, scope changes, and
  anything [PLAN.md §6](docs/PLAN.md#6-resolved-follow-ups-and-remaining-user-decisions) lists as open. Ask, then
  record the answer in `docs/design/04-user-decisions.md`.

## Repository map

| Path | What |
|---|---|
| `docs/` | Brief, architecture, plan, verification, rot register; `coolprop-map/` and `research/` evidence; `design/` history and the type sketch |
| `scripts/` | `fetch-coolprop.sh`, `check-toolchain.sh`, `oracle/gen.py` (the fixture generator), `baseline/` (the C++ CoolProp baseline) |
| `crates/phasekit-*` | the workspace: core, data, verify, xtask; compat at M5.9; capi, wasm and py later |
| `reference/` | gitignored: the CoolProp checkout and local papers |
