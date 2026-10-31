# Target directories and machines (2026-10-31)

Asked while freeing disk space on the laptop: 69 `~/.cache/phasekit-target-*` directories, one `CARGO_TARGET_DIR` per
task, held about 285 GB of its 473 GB disk (20-37 GB each by `du`), none used since 2026-10-17. Nothing removed them.
The laptop's `main` was also 81 commits behind origin's.

| # | Question | Decision | Notes |
|---|---|---|---|
| SP3 | Full debug info in the dev profile? | **No, line tables only** ("Setting [profile.dev] debug = "line-tables-only" there would also make each directory much smaller") | `Cargo.toml`'s `[profile.dev]`, and so `test`; `mutants` has none. Panics and backtraces keep their files and lines; a debugger loses local variables and types. The workspace's test build (`cargo test --workspace --no-run`, cold, on the laptop): 3.8 GB in 198 s, against 6.5 GB in 238 s with full debug info (`du`). |
| SP4 | A target directory per task? | **No: sessions reuse a few shared ones** ("have its sessions reuse a few shared target directories instead of making one per task") | `scripts/target-dir.sh`, three by default, which every command that runs cargo calls. A directory serves one checkout at a time: cargo judges a build fresh by file times, so a checkout whose files are older than another checkout's build in the same directory ran that build (shown with cargo 1.99), and the script deletes the workspace crates' fingerprints when a directory changes hands. It also cleans up, so that no machine needs the laptop's daily timer (outside the repository): once a day it removes `phasekit-target-*` directories unused for 7 days, as the timer does, and empties the directory it hands out once that passes 20 GB ("yes, lets try with 20GB"), as cargo never removes what an old toolchain or dependency built. |
| MM1 | How does a session know it has the latest work? | **It fetches origin and starts from `origin/main` before starting anything new** ("make sure you have latest origin code before starting new work as I develop this on at least 2 machines") | AGENTS.md "Where the work is". |
