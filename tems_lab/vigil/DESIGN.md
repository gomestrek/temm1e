# Tem Vigil — Self-Diagnosing Bug Reporter

> **Date:** 2026-04-04
> **Status:** Design phase
> **Branch:** tem-vigil

---

## Problem

1. Users hit bugs and have no easy way to collect/share debug info. Logs go to stdout unless manually redirected.
2. Non-developer users can't diagnose issues or file structured bug reports.
3. The TEMM1E team discovers bugs only when users manually report them on Discord/Telegram — often with insufficient context.

## Solution: Two-Layer System

### Layer 0: Centralized Log File (always on, local only)

All tracing output written to `~/.temm1e/logs/temm1e.log` with daily rotation and 7-day retention. No user config needed. No network. No privacy risk.

**Implementation:** Add `tracing-appender` (rolling file appender) alongside the existing stdout subscriber. Use `tracing_subscriber::layer` to multiplex both outputs.

**User benefit:** "Attach `~/.temm1e/logs/temm1e.log` to your GitHub issue" — one sentence, works for anyone.

### Layer 1: Auto Bug Reporter (opt-in, user-controlled)

Tem monitors its own logs during Perpetuum Sleep phase. When it detects recurring ERROR patterns (panics, provider failures, tool crashes), it:

1. **Triages** via LLM call: "Is this a real bug or user error/misconfiguration?"
2. **Sanitizes** via existing `credential_scrub.rs`: strips API keys, tokens, file paths with usernames
3. **Deduplicates** via GitHub API search: checks if an open issue with the same error signature exists
4. **Previews**: shows the user exactly what will be sent
5. **Creates** a GitHub issue on `temm1e-labs/temm1e` with `[BUG]` label

**Auth:** User provides a GitHub PAT via `/addkey github`. Stored encrypted in vault. PAT needs only `public_repo` scope (covers issue creation on public repos). No bot token shipped in the binary.

**Consent model:**
- Layer 0 (local logs): always on, no consent needed
- Layer 1 (GitHub reporting): explicit opt-in required
  - First bug detected: Tem asks "I found a bug in myself. Want me to report it to my developers? I'll show you the report first."
  - User can disable permanently: `[bug_reporter] enabled = false`
  - Default: `enabled = true`, `consent_given = false` (will ask on first bug)

---

## Architecture

```
                      ┌─────────────────────────────┐
                      │    Layer 0: Log File         │
                      │    ~/.temm1e/logs/temm1e.log │
                      │    Always on. Local only.    │
                      └──────────┬──────────────────┘
                                 │
                    ┌────────────▼────────────────┐
                    │  Perpetuum Sleep Phase       │
                    │  Concern: bug_review         │
                    │  Schedule: every 6 hours     │
                    └────────────┬────────────────┘
                                 │
                    ┌────────────▼────────────────┐
                    │  Log Scanner                 │
                    │  Filter: ERROR + WARN        │
                    │  Window: last 6 hours        │
                    │  Dedup: same error sig once   │
                    └────────────┬────────────────┘
                                 │
                    ┌────────────▼────────────────┐
                    │  LLM Triage                  │
                    │  "Is this a real bug?"        │
                    │  Classify: bug / user-error   │
                    │  / config / transient         │
                    └────────────┬────────────────┘
                                 │ (only if bug)
                    ┌────────────▼────────────────┐
                    │  Credential Scrub            │
                    │  Strip: API keys, tokens,    │
                    │  passwords, usernames in      │
                    │  paths, message content       │
                    └────────────┬────────────────┘
                                 │
                    ┌────────────▼────────────────┐
                    │  GitHub Dedup               │
                    │  Search open issues for      │
                    │  same error signature         │
                    └────────────┬────────────────┘
                                 │ (only if new)
                    ┌────────────▼────────────────┐
                    │  User Preview + Consent      │
                    │  "I want to report this bug. │
                    │   Here's what I'll send..."   │
                    └────────────┬────────────────┘
                                 │ (only if consented)
                    ┌────────────▼────────────────┐
                    │  GitHub Issues API            │
                    │  POST /repos/.../issues       │
                    │  Auth: user's PAT from vault  │
                    │  Label: [BUG], auto-generated │
                    └─────────────────────────────┘
```

---

## What Gets Sent (Issue Template)

```markdown
## [BUG] Panic in context.rs: byte index not a char boundary

**Auto-reported by Tem v4.1.2 on 2026-04-04**

### Error
```
panic: byte index 200 is not a char boundary in `...`
  at crates/temm1e-agent/src/context.rs:407
```

### Context
- Version: 4.1.2
- OS: Darwin 23.6.0 (aarch64)
- Rust: 1.82
- Provider: gemini (gemini-3-flash-preview)
- Channel: telegram
- Occurred: 3 times in last 6 hours

### Triage
Category: Panic (caught by catch_unwind)
Severity: High — affects message processing
Pattern: Occurs on messages containing multi-byte UTF-8 characters

### System Info
- Crates: 20
- Uptime: 4h 23m
- Sessions active: 2
- Memory backend: sqlite

---
*This issue was automatically generated by Tem's self-diagnosis system.
User has reviewed and approved this report before submission.*
```

**What is NOT sent:** API keys, user messages, conversation history, vault contents, file paths with usernames, IP addresses, credentials of any kind.

---

## What Gets Scrubbed

Using existing `credential_scrub.rs` patterns plus:

| Pattern | Action | Example |
|---|---|---|
| `sk-ant-*`, `sk-or-*`, `sk-*` | Replace with `[REDACTED_API_KEY]` | `sk-ant-abc123` → `[REDACTED_API_KEY]` |
| `AIza*` (Google keys) | Replace with `[REDACTED_API_KEY]` | `AIzaSy...` → `[REDACTED_API_KEY]` |
| `xai-*` (xAI keys) | Replace with `[REDACTED_API_KEY]` | `xai-abc...` → `[REDACTED_API_KEY]` |
| `/Users/<name>/`, `/home/<name>/` | Replace with `~/` | `/Users/john/...` → `~/...` |
| `Bearer *`, `token=*` | Replace with `[REDACTED_TOKEN]` | |
| Env var values from `.env` | Replace with `[REDACTED_ENV]` | |
| Chat message content | Never included | |
| Vault entries | Never included | |

---

## Security Analysis

| Threat | Mitigation |
|---|---|
| PAT leaked from vault | Vault uses ChaCha20-Poly1305 encryption. PAT only needs `public_repo` scope — can create issues, nothing else |
| Sensitive data in bug report | `credential_scrub.rs` + additional patterns. User previews before send. |
| Spam/abuse | Rate limit: max 1 issue per 6 hours. Dedup against existing open issues. |
| LLM hallucinated diagnosis | Issue body contains raw facts (stack trace, OS, version). LLM triage only classifies severity — does NOT propose root causes. |
| Reporter bug causes more reports | Reporter itself uses `catch_unwind`. Failures in the reporter are logged locally but never trigger another report. |
| User doesn't want this | Default consent_given=false. Explicit opt-in. Config to disable entirely. |
| GDPR | No data collected without consent. No data sent to third parties (GitHub is user's own account). User controls the PAT. |

---

## Perpetuum Integration

```rust
// Register during Sleep phase
Concern {
    name: "bug_review",
    kind: ConcernKind::Recurring,
    interval: Duration::from_secs(6 * 3600), // every 6 hours
    phase: PerpetualPhase::Sleep,
    handler: bug_review_handler,
}
```

The handler:
1. Reads `~/.temm1e/logs/temm1e.log` for last 6 hours
2. Filters ERROR/WARN/panic lines
3. Groups by error signature (file:line + error message prefix)
4. For each unique error group with count >= 2:
   a. LLM triage call
   b. If classified as bug: scrub, dedup, preview, create

---

## Crate Structure

No new crate. Two additions:

1. **`crates/temm1e-observable/src/file_logger.rs`** — Layer 0 file appender setup
2. **`crates/temm1e-perpetuum/src/bug_reporter.rs`** — Layer 1 auto-reporter + LLM triage

Why no new crate: logging belongs in observable (already has tracing setup), bug reporting is a Perpetuum concern (it runs during Sleep). Both are small additions (~100-150 LOC each).

---

## Is This Worth a tems_lab Release?

**Yes, for two reasons:**

### 1. The centralized log file is table-stakes

Every production runtime has this. TEMM1E currently loses all logs unless the user manually redirects stdout. This is a critical gap for any user trying to debug issues. The log file alone justifies the release.

### 2. Self-diagnosing AI is a genuinely novel capability

From research: self-healing infrastructure is established (Azure, AWS), and LLM-powered bug triage is emerging (Microsoft Triangle, openSUSE). But **an AI agent that monitors its own execution logs, identifies bugs in itself via its own LLM, and files bug reports about itself to its own GitHub repo** — this is frontier territory. No existing system does this.

The research paper angle: this is Tem becoming **self-aware of its own failures**. Consciousness watches the agent think. The bug reporter watches the agent fail. Together, they form a metacognitive stack: think, observe, diagnose.

### What this is NOT

- Not a crash analytics platform (no server, no dashboards)
- Not a replacement for Sentry (no event aggregation, no release tracking)
- Not automatic fixing (Tem reports bugs, humans fix them)

It's the simplest possible thing: **Tem notices when something goes wrong and tells its developers, with the user's permission.**

---

## Implementation Plan

| Phase | What | LOC | Risk |
|---|---|---|---|
| 1 | Layer 0: file logger with rotation | ~50 | Zero |
| 2 | `/addkey github` PAT support | ~30 | Zero (extends existing flow) |
| 3 | Log scanner + error grouping | ~80 | Zero (read-only) |
| 4 | LLM triage prompt | ~40 | Low (uses existing provider) |
| 5 | Credential scrub extension | ~30 | Zero (extends existing scrubber) |
| 6 | GitHub issue creation + dedup | ~100 | Low (rate-limited, user-consented) |
| 7 | Perpetuum Sleep concern | ~40 | Zero (standard concern registration) |
| 8 | User consent flow | ~50 | Zero (config + message) |
| **Total** | | **~420 LOC** | **Low** |
