# Readable fluid data (2026-10-06)

Asked during M2 (M2.4 under way): the blobs are binary, so neither a human nor an agent can read or check the data
that ships. The user asked for a readable form in M2.

| # | Question | Decision | Notes |
|---|---|---|---|
| FD1 | A readable, checkable form of the fluid data | **Yes: PLAN.md M2.9a, `cargo xtask fluid list\|show\|diff`** | JSON from the decoded blobs, deterministic, full float precision; in xtask (serde_json is already allowed there, tier T4), so no new crate or dependency. Its key test: the dump parses and re-encodes to the blob byte for byte for all 136 fluids, so the readable form is complete (and the natural input for authoring a fluid later). `every_blob_section_is_dumped` plus a PLAN.md §2.5 rule make every later section-filling step extend the dump. Optional, if it needs no new permission: the CI `linux` job writes `fluid diff origin/main` to its job summary on PRs that change data. |
