---
name: never-send-email
description: "Never send the user's email address to any external service without asking first"
metadata:
  node_type: memory
  type: feedback
  modified: 2026-10-04T16:38:27.898Z
---

Never send the user's email address to anything external (APIs, URLs, query strings, headers, web forms) without checking with them first and getting an explicit yes for that use. The rule also lives in ~/.claude/CLAUDE.md so subagents inherit it, and is enforced by a user-level PreToolUse hook (~/.claude/hooks/guard-email.sh) that turns any non-file tool call containing the address into a permission prompt.

**Why:** on 2026-10-04 a fact-checking subagent in the coolprop-rs workflow put the user's email in an Unpaywall API query string; the user asked for this to NEVER happen again.

**How to apply:** for APIs that request a contact email (Unpaywall, OpenAlex, Crossref, NCBI), omit it or use an anonymous endpoint; if one is truly required, stop and ask. State the rule explicitly in subagent/workflow prompts that do web research. Related: [[coolprop-rs-plan-status]]
