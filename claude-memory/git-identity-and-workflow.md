---
name: git-identity-and-workflow
description: "Commit identity (GitHub noreply only, never personal email) and the user's git workflow rules for phasekit"
metadata:
  node_type: memory
  type: feedback
  modified: 2026-10-05T09:13:31.442Z
---

Commits must use the user's GitHub identity only: `Dzoay <3277116+Dzoay@users.noreply.github.com>` (GitHub login Dzoay, id 3277116), set as repo-local git config. Never put the personal email in commits, trailers or tags. There is no global git identity on this machine, so always set it per repo before the first commit.

Workflow rules the user asked for (2026-10-05), to live in AGENTS.md / README / CONTRIBUTING: linear history (first asked as "fast-forward PRs"; after the GitHub trade-offs were explained the user chose squash merge only), Conventional Commits, SemVer. GitHub + Actions, public repo `Dzoay/phasekit`; `gh` is authenticated and SSH is set up, and the user authorised creating the repo.

Repo: https://github.com/Dzoay/phasekit (public), local clone ~/Projects/phasekit (renamed from ~/Projects/coolprop-rs on 2026-10-05). Settings: squash merge only (title=PR_TITLE, body=PR_BODY), delete branch on merge, topics set, private vulnerability reporting on, labels incl. `divergence`. Ruleset 'main' (id 24496175): PR required (0 approvals, squash only), linear history, no force-push, no deletion, **required signatures**, required status checks (integration 15368 = GitHub Actions): "Conventional PR title" (pr-title.yml) plus, since M0.6 (user approved 2026-10-05), the ci.yml jobs fmt, clippy, linux, msrv, windows, aarch64, wasip2, wasm-browser; no bypass. Ruleset edits: GET the ruleset, change only the required_status_checks rule, PUT the full body back. PR #1 merged as 9ee4ac9 (GitHub-signed, Verified).

Signing and SSH (set up 2026-10-05): repo-local `gpg.format ssh`, `user.signingkey ~/.ssh/id_ed25519.pub`, `commit.gpgsign`/`tag.gpgsign true`, allowed signers in .git/allowed_signers; the key is registered on GitHub as a signing key (id 1220119). The passphrase-protected key lives in GnuPG's agent (SSH socket /run/user/<uid>/gnupg/S.gpg-agent.ssh, cache 8 h via ~/.gnupg/gpg-agent.conf); ~/.bashrc and fish config export SSH_AUTH_SOCK to it, so SSH push and signing work from Claude's shell (export it explicitly if a shell lacks it). If signing fails, ask the user to `ssh-add` again; never disable signing. GitHub refuses squash merges of PRs containing unsigned commits. Squash merges done by GitHub are authored the profile's display name with Dzoay@users.noreply.github.com (profile display name), committer GitHub.

CI (M0.6): .github/workflows/{ci,weekly,nightly}.yml; stable toolchain pinned once in .github/actions/rust/action.yml; GitHub actions pinned by SHA; tools cached in ~/.tools. Finished job logs before the run completes: `gh api --allow-escape-sequences repos/Dzoay/phasekit/actions/jobs/<id>/logs`. CI sets CARGO_TERM_COLOR=always; xtask forces never for parsed cargo output.

Lesson: when chaining `gh pr merge` with follow-up commands, check its exit status first (a blocked merge once let a chain delete the local branch).

**Why:** the user explicitly said "Please do NOT use my person email for commits, github identity ONLY" — consistent with [[never-send-email]].

**How to apply:** before any commit check `git config user.email` is the noreply address; in subagent prompts that commit, state the identity explicitly. Related: [[coolprop-rs-plan-status]]
