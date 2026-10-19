# AGENTS.md: instructions for coding agents working on phasekit

phasekit is a from-scratch, idiomatic Rust successor to CoolProp (thermophysical properties of fluids, later
materials). It is **not** a binding. Work proceeds test-first, one plan step per pull request.

**Where the work is:** read it from git, never from a doc. `git tag` lists the closed milestones (`m0` …);
`phasekit_verify::MILESTONE` is the first open one; the next step is the first in [docs/PLAN.md](docs/PLAN.md) §3
with no `Plan-Step:` commit on `main` (`git log --oneline --grep '^Plan-Step:'`).

## Sources of truth, in order

1. [docs/BRIEF.md](docs/BRIEF.md) §1: the user's requirements, verbatim. They are the contract.
2. [docs/decisions/](docs/decisions/): the user's decisions, a file each time the user answers, each decision by its id
   ([docs/DECISIONS.md](docs/DECISIONS.md) says how). They override anything older.
3. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): the design, decisions D1-D17, seams and invariants. For what is
   built, the code in `crates/` is the truth.
4. [docs/PLAN.md](docs/PLAN.md): the steps, gates and what is open. [docs/VERIFICATION.md](docs/VERIFICATION.md):
   fixtures, tolerances, the divergence workflow, CI. [docs/ROT-REGISTER.md](docs/ROT-REGISTER.md): CoolProp problems
   designed out, each with its proof.

Each fact has one home ([PLAN.md §2.1](docs/PLAN.md#21-ids-and-references), which also defines the citation forms):
change it there and link to it, never copy it. If two sources disagree, follow the higher one and fix the lower one in
the same PR, or stop and ask.

**Evidence and history are not instructions.** `docs/coolprop-map/` (CoolProp v8.0.0) and `docs/research/` are frozen
evidence: open them only to resolve a citation that a living doc or the step gives, and read only that section. Their
recommendations, rankings, type names and open questions were inputs to decisions that sometimes went the other way;
where they disagree with the documents above, the documents above win (say so in the PR). `docs/history/` is the
frozen record of how the design was chosen: open it only to resolve an S-## or E## id, or when the user asks.

## How to work a step

Follow [PLAN.md §0](docs/PLAN.md#0-how-to-use-this-plan): write the named failing test first from its oracle or
arbiter, show it red, implement, make all gates green, collapse the step to its ledger row (§0.1 item 7), open the
PR. If a step fails its gate in
a way the plan did not foresee, stop and re-plan ([PLAN.md §0.4](docs/PLAN.md#04-when-a-step-fails-its-gate-stop-and-re-plan)).
Do not start the next step in the same PR. `scripts/check-toolchain.sh` checks that the tools the gates need are
installed. After editing `.github/workflows/`, run `actionlint` (no gate does) and fix what it reports.

Gates are defined once in [PLAN.md §2.4](docs/PLAN.md#24-standard-gates) (G1-G9). Agents always set
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
  AGENTS.md are for CoolProp contributors and do not apply here. So is `reference/AGA8` (NIST's AGA8 code, pinned by
  `scripts/fetch-aga8.sh`; GERG-2008's source, user decision MX7), and so is `reference/optimized-routines` (Arm's,
  pinned by `scripts/fetch-optimized-routines.sh`; the source of core's in-house maths, user decision IM5).
- **Oracle:** `uv run --no-project --python 3.12 --with CoolProp==8.0.0 python …`. CoolProp is a *provisional* oracle;
  printed literature arbitrates. **Never widen a tolerance to make CoolProp pass.** A disagreement goes through the
  divergence workflow ([VERIFICATION.md §6](docs/VERIFICATION.md#6-divergence-register)).
- **Dependencies:** none unless the step names one and it fits the tiers in [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md).
  `phasekit-core` uses std only until M9.
- **Safety:** `unsafe` only in `phasekit-capi`. No panics on user input (`todo!`, `unimplemented!`, `unwrap`,
  `expect` are denied by lints). No global mutable state in the kernel.
- **Papers and data:** PDFs of papers live in `reference/papers/` (gitignored) and are never committed. Only
  transcribed check values with citations are committed, as the plan describes. Restricted data (map 09 §9) never
  enters default features.
- **Do not edit `docs/history/`.** It is the frozen record of how the design was chosen, the type sketch included
  ([its README](docs/history/README.md)); it uses the old placeholder names `cprs-*`/`cp_*` on purpose.
  A file joins `docs/decisions/` only when the user decides something, named for that date (`<date>-<slug>.md`).
- **Decisions that belong to the user** stop the step: naming, licensing, publishing, accounts, scope changes, and
  anything [PLAN.md §6](docs/PLAN.md#6-resolved-follow-ups-and-remaining-user-decisions) lists as open. Ask, then
  record the answer in a new file under `docs/decisions/`, never in `docs/DECISIONS.md` (`gates decisions`).

## Repository map

| Path | What |
|---|---|
| `docs/` | brief, decisions, architecture, plan, verification, rot register, dependencies, migration; `coolprop-map/` and `research/` (frozen evidence); `history/` (frozen, with the type sketch) |
| `scripts/` | `fetch-coolprop.sh`, `fetch-aga8.sh`, `fetch-optimized-routines.sh`, `check-toolchain.sh`, `oracle/gen.py` (the fixture generator), `oracle/teqp.py` (GERG-2008's), `math/reference.py` (the maths reference values), `baseline/` (the C++ CoolProp baseline), `release/` (the release artefacts) |
| `crates/phasekit-*` | the workspace: core, data, verify, xtask, compat, capi, wasm; py later |
| `reference/` | gitignored: the CoolProp and AGA8 checkouts and local papers |
