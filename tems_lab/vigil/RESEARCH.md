# Tem Vigil: Self-Diagnosing AI Agent — Research Notes

> **Date:** 2026-04-04
> **Status:** Research complete, design approved

---

## Prior Art

### Crash Reporting Systems (Sentry, Bugsnag, Crashlytics)

Standard architecture: client SDK captures panic/exception with stack trace + system metadata, POSTs to ingestion endpoint, server groups by fingerprint, creates dashboard entry. Sentry DSNs are write-only by design — safe to expose publicly. Self-hosted Sentry requires 20+ containers (Kafka, ClickHouse, PostgreSQL, Redis). Overkill for single-project use.

**Relevant pattern:** Sentry's DSN model proves that write-only tokens are safe to expose. The Rust `crashreport` crate hooks `std::panic::set_hook` to generate pre-filled GitHub issue URLs — simple, no server needed.

### GitHub Bot Issue Creation (Dependabot, Renovate, CodeQL)

All use GitHub Apps with short-lived installation tokens. Fine-grained permissions: `Issues: write` + `Metadata: read` is sufficient. Rate limit: 15K req/hr for Apps, 5K for PATs. GitHub Apps require a server-side component for token exchange — not viable for a self-hosted binary.

**Relevant pattern:** Fine-grained PATs scoped to `public_repo` can create issues on public repos. The blast radius is exactly: can create/edit issues. Cannot read code, push, access secrets.

### LLM-Powered Bug Triage

Microsoft's Triangle System uses LLM agents for Azure incident triage — correlating logs with code, recommending assignments, reducing mean-time-to-triage. ISSRE 2024 research shows LLMs achieve accurate triage on real incident datasets. openSUSE built an LLM-Bugzilla integration for automatic bug classification.

**What works:** Severity classification, reproduction step extraction, deduplication via semantic similarity, human-readable summaries. Log parsing accuracy reaches 0.96 with modern approaches.

**What fails:** Hallucinated root causes. LLMs confidently propose incorrect diagnoses. Without grounding in actual code/git history, root cause analysis is unreliable.

**Relevant pattern:** Use LLM for triage (is this a bug?) and summary (what happened?), NOT for root cause analysis. Present raw facts in the issue body. Let humans diagnose.

### Privacy in Automated Reporting

ChromeOS: opt-in consent during setup, queued crashes deleted if consent revoked. Firefox crash reporter: GDPR violation flagged for collecting before consent. Industry standard: explicit opt-in, preview before send, never include credentials or PII.

**Relevant pattern:** TEMM1E handles API keys, conversation content, and vault secrets. The reporter MUST: strip all credentials, redact usernames from paths, never include message content, require explicit opt-in, show preview.

### Client-Side Tokens in Open Source

**Do NOT ship tokens in open-source binaries.** DogWifTool incident: GitHub token extracted from distributed binary, used maliciously. Sentry's model works because DSNs only permit writes — but GitHub PATs, even fine-grained, can be used for other write operations within their scope.

**Relevant pattern:** Two safe options: (a) users provide their own PAT, or (b) project runs a write-only webhook endpoint. Option (a) requires zero infrastructure.

### Self-Diagnosing AI Systems

Azure VMware: closed-loop control (detect → diagnose → act → verify). AWS DevOps Agent: autonomous incident response. LogicMonitor Edwin: detection-through-resolution.

**The novel angle:** An AI agent that monitors its own `catch_unwind` results, detects recurring panic patterns, classifies them via its own LLM, and files bug reports about itself. Self-healing infrastructure is established. Self-reporting AI agents are not.

---

## Key Decision: No Embedded Token

Original proposal: ship a TEMM1E-owned bot token in the binary.

**Rejected.** Research conclusively shows this is unsafe for open-source projects. Even with XOR obfuscation, the token is extractable. Even scoped to issues-only, it creates a shared resource vulnerable to abuse.

**Adopted approach:** Users provide their own GitHub PAT via the existing `/addkey` flow. The PAT is stored encrypted in the vault. Zero infrastructure, zero shared secrets, user controls their own auth.

For non-developer users who don't have GitHub: Layer 0 (local log file) is always available. They can share `~/.temm1e/logs/temm1e.log` via any channel (Discord, Telegram, email). Layer 1 (auto-reporting) is a convenience for users who do have GitHub.

---

## Novelty Assessment

| Capability | Exists? | Who? |
|---|---|---|
| Centralized log file with rotation | Common | Every production system |
| Crash capture with stack trace | Common | Sentry, Bugsnag, crashreport-rs |
| LLM-powered bug triage | Emerging | Microsoft Triangle, openSUSE |
| Auto-create GitHub issues from crashes | Rare | crashreport-rs (URL only, not automated) |
| AI agent self-diagnosing its own failures | **Novel** | No known system |
| AI agent filing bugs about itself | **Novel** | No known system |
| Perpetuum lifecycle integration (Sleep → diagnose) | **Novel** | Unique to TEMM1E |

The combination of self-diagnosis + self-reporting + lifecycle-aware scheduling is genuinely new. Individual components exist, but the integrated system does not.

---

## References

- Sentry Architecture: develop.sentry.dev/application-architecture/overview/
- Sentry DSN Security: sentry.zendesk.com/hc/en-us/articles/26741783759899
- GitHub App Permissions: docs.github.com/en/rest/authentication/permissions-required-for-github-apps
- Microsoft Triangle (Azure AIOps): azure.microsoft.com/en-us/blog/optimizing-incident-management-with-aiops-using-the-triangle-system/
- LLMs for Incident Triage (ISSRE 2024): microsoft.com/en-us/research/wp-content/uploads/2024/08/ISSRE24_LLM4triage.pdf
- openSUSE LLM Bug Triage: news.opensuse.org/2025/11/19/hw-project-targets-bug-triage/
- AWS Agentic DevOps Agent: aws.amazon.com/blogs/devops/leverage-agentic-ai-for-autonomous-incident-response/
- ChromeOS Crash Reporter: chromium.googlesource.com/chromiumos/platform2/crash-reporter/README.md
- Rust crashreport crate: github.com/ewpratten/crashreport-rs
- Self-Healing AI Patterns: dev.to/the_bookmaster/the-self-healing-agent-pattern
