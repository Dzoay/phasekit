# Error codes in core (2026-10-22)

Asked after a review of M28.1's messages, before it was pushed. The review asked whether each message is accurate,
says what the caller can do, suits an agent using the library (through an MCP server, say) and helps a simulator
search for a physical state at high call rates. An agent and a simulator are served by the structured error, not its
text. Two facts were then inside the facades or private: each error's machine code (the browser package's
`below_min_temperature`, `invalid_input`, … in `phasekit-wasm`) and the rule an invalid value breaks (computed by the
message, private), so every facade would have to rebuild them. Five follow-ups were proposed: the codes and the rule
in core, remedy codes, the reachable bound and side of an "out of range" value, detail in batch results, and wording
fixes.

| # | Question | Decision | Notes |
|---|---|---|---|
| EM11 | Does core own each error's code and expose the rule an invalid value breaks, and when? | **Yes, in M28.1: `Error::code()`, a stable snake_case code for each variant and each `DomainError`, and `Error::rule()`, the `Rule` an `InvalidInput`'s value breaks, with its own code** | "put 1 in M28.1, file an issue for 2-5". The codes are those the browser package had (`below_min_temperature`, `invalid_input`, …), so its refusals keep them; M28.2 reads them from core. The other four follow-ups (remedy codes, the reachable bound and side, batch detail, wording fixes) are issue #290, each for a later decision. |
