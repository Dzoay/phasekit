# Contributing to phasekit

This file is the one definition of how changes land. [AGENTS.md](AGENTS.md) points here, and
[docs/PLAN.md §2.2](docs/PLAN.md#22-branches-commits-prs-versions-toolchain) summarises it per step.

## The short version

1. Pick the next step in [docs/PLAN.md](docs/PLAN.md) (or an issue).
2. Branch from an up-to-date `main`: `m3.2-power-oracle` for a plan step, `<type>/<slug>` otherwise.
3. Write the failing test first, commit it, then make it pass ([PLAN.md §0.1](docs/PLAN.md#01-the-step-loop)).
4. Run the standard gates locally ([PLAN.md §2.4](docs/PLAN.md#24-standard-gates)).
5. Open a PR whose **title is a Conventional Commit**. It is squash-merged when CI is green.

## History: linear, one commit per PR

- `main` only changes through pull requests, and **squash merge is the only merge method**. Each PR becomes exactly one
  commit on `main`, so history is linear and every commit on `main` built and passed CI.
- Keep a PR to one focused change: one plan step, one fix, one refactor. Unrelated cleanups get their own PR.
- Before merging, rebase the branch on `main` (`git pull --rebase origin main`) so CI tests what will land.
- Never force-push or delete `main`; the repository ruleset forbids both. Force-pushing your own PR branch is fine.
- Commits inside a PR branch can be as small and messy as you like; only the PR title and description survive.
  The **PR title becomes the commit subject and the PR description becomes the commit body**, so keep the description
  commit-worthy: what and why, the red evidence, then the footers (the PR template has exactly these parts). The
  checklist below lives here, not in the PR.

## Commit messages: Conventional Commits 1.0

The PR title becomes the commit subject on `main`, so it must follow
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```text
<type>(<scope>)<!>: <imperative summary, no trailing full stop>

<body: what and why; the red evidence for a test-first step>

Plan-Step: M3.2
Rot: ROT-074
Div: DIV-0012
BREAKING CHANGE: <what breaks and how to migrate>
```

| Type | Use for | Version effect (§ SemVer) |
|---|---|---|
| `feat` | new user-visible capability | minor (patch before 1.0) |
| `fix` | a bug fix | patch |
| `perf` | faster, same results (bitwise, where the scalar reference applies) | patch |
| `refactor` | restructuring with no behaviour change | none |
| `test` | tests or fixtures only | none |
| `docs` | documentation only | none |
| `build`, `ci` | Cargo, toolchain, workflows | none |
| `chore` | everything else (dependency bumps, `chore: rust 1.NN`) | none |
| `revert` | reverts an earlier commit (`revert: feat(core): ...`) | as the reverted change |

- **Scopes:** the crate without its prefix (`core`, `data`, `compat`, `verify`, `xtask`, `capi`, `wasm`, `py`), or
  `plan`, `deps`, `repo`. Omit the scope only when a change truly spans everything.
- **Breaking changes:** add `!` before the colon *and* a `BREAKING CHANGE:` footer.
- **Footers:** `Plan-Step:` is required on plan steps. `Rot:` and `Div:` link rot-register rows and divergence-register
  entries.
- **Checked by CI:** the `pr-title` workflow (`.github/workflows/pr-title.yml`) fails a PR whose title is not
  `<type>(<scope>)<!>: <summary>` with a listed type and scope, a summary with no trailing full stop, and at most 72
  characters. A new crate or scope is added to that workflow and to the list above in the same PR.

Examples:

```text
feat(core): power-term jets match the oracle
fix(data)!: use the paper's gas constant for R1234ze(E)
docs(plan): split M6.3 into saturation and critical-point steps
```

## Identity

Commit under your **GitHub identity only**: your GitHub username and its no-reply address
(`<id>+<username>@users.noreply.github.com`, shown under GitHub → Settings → Emails). Never commit with a personal
email address. Set it per repository:

```sh
git config user.name  "<github-username>"
git config user.email "<id>+<github-username>@users.noreply.github.com"
```

Turning on "Keep my email addresses private" and "Block command line pushes that expose my email" in GitHub's email
settings makes GitHub reject a push that would leak it.

### Signed commits

Every commit on `main` must be signed; the `main` ruleset enforces it. Commits on `main` are squash merges that GitHub
creates and signs itself, so they are always "Verified". Sign your own commits and tags too, with your SSH key:

```sh
git config gpg.format ssh
git config user.signingkey ~/.ssh/id_ed25519.pub
git config commit.gpgsign true
git config tag.gpgsign true
```

Add the same public key on GitHub (Settings → SSH and GPG keys → New SSH key, key type **Signing key**). Release tags
`vX.Y.Z` are always signed.

## Versions: SemVer 2.0

- All published crates share one version (`[workspace.package] version` in the root `Cargo.toml`).
- **Before 1.0:** a breaking change bumps the minor version (0.1.x → 0.2.0); everything else bumps the patch.
  **From 1.0:** standard SemVer (breaking → major, `feat` → minor, `fix`/`perf` → patch).
- The bump follows from the Conventional Commits since the last tag: any `!` → breaking; otherwise any `feat` → minor
  (patch before 1.0); otherwise patch.
- `cargo semver-checks` must pass before every release; it catches an API break the commit types missed.
- Releases are signed, annotated tags `vX.Y.Z` on `main`. `CHANGELOG.md` is generated from the commit subjects since the last
  tag. Crates (crates.io) and the browser package (npm) are published from the tag by GitHub Actions using trusted
  publishing; no API tokens live in the repository.
- Versioned separately, never by the crate version: the CoolProp v8.0.0 codes in the C compatibility shim (pinned) and
  the `phasekit-data` dataset id.

## What a PR must show

- For a test-first step: the failing test and its red output (paste it in the PR description).
- All standard gates green: G1-G8 locally, and the `ci` workflow's required checks on the PR.
- Docs that the change affects updated in the same PR (PLAN.md, VERIFICATION.md, ROT-REGISTER.md, ARCHITECTURE.md).
- When results differ from CoolProp: the divergence workflow in
  [VERIFICATION.md §6](docs/VERIFICATION.md#6-divergence-register). Never widen a tolerance to make CoolProp pass.
- No new dependency unless the step names it and it fits the tiers in
  [docs/research/dependencies.md §3.1](docs/research/dependencies.md).

## Licence

By contributing you agree that your contribution is licensed under MIT OR Apache-2.0, like the rest of the project
([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
