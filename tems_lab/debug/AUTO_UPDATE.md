# Tem Debug: Auto-Update — Risk Analysis and Design

> **Date:** 2026-04-04
> **Status:** Research complete. HIGH RISK — requires careful design.
> **Related:** Bug reporter (auto-issues) + auto-update = closed feedback loop

---

## Current State

`temm1e update` (main.rs:5885-6015) does: git fetch → git pull → cargo build --release. This only works for users who cloned the repo and have Rust installed. Binary users (install.sh, Docker, GitHub Releases) have no update path.

---

## The Vision: Closed Feedback Loop

```
Tem finds bug → files GitHub issue → developers fix it → new release tagged
                                                              ↓
Tem checks for updates ← GitHub Releases API ← new version available
                                                              ↓
Tem downloads new binary → verifies signature → notifies user
                                                              ↓
User restarts (or Tem self-restarts during idle) → bug is fixed
```

The auto-issue system (Tem Debug Layer 1) and auto-update complete a cycle: Tem reports its own bugs and receives its own fixes.

---

## Installation Methods and Update Paths

| Install method | Current update | Auto-update viable? |
|---|---|---|
| `git clone` + `cargo build` | `temm1e update` (git pull + rebuild) | Already works |
| `install.sh` (curl binary) | Re-run install.sh manually | **Yes** — replace binary via GitHub Releases |
| Docker | `docker pull` manually | **Yes** — watch for new image tags |
| Homebrew (future) | `brew upgrade temm1e` | Handled by Homebrew |
| npm (not applicable) | N/A | N/A |

The gap is **binary users** who used install.sh. They have a binary at `/usr/local/bin/temm1e` (or `~/.temm1e/bin/temm1e`) with no source tree and no package manager.

---

## Technical Design

### Check for Updates

```rust
// Perpetuum concern: check every 6 hours during Idle/Sleep
// GET https://api.github.com/repos/temm1e-labs/temm1e/releases/latest
// Compare tag_name (e.g., "v4.1.2") against env!("CARGO_PKG_VERSION")
// If newer: log + notify user via active channel
```

**No auth required.** GitHub public API rate limit for unauthenticated: 60 req/hr. We make 1 check every 6 hours = 4/day. Well within limits. If the user has a GitHub PAT (from bug reporter), use it for 5000 req/hr.

### Download + Verify

```
1. Determine target triple: {os}-{arch} (e.g., x86_64-unknown-linux-musl)
2. Find matching asset in release (e.g., temm1e-x86_64-unknown-linux-musl.tar.gz)
3. Download asset + .sha256 checksum file
4. Verify SHA256 matches
5. (Future) Verify Ed25519 signature of the binary
6. Stage to ~/.temm1e/updates/temm1e-{version}
7. Notify user: "Update v4.1.3 ready. Restart to apply."
```

### Apply Update

**NOT automatic restart.** The daemon holds active connections (Telegram long-poll, Discord gateway, WhatsApp Web WebSocket). Restarting drops all of them. Channels reconnect automatically, but there's a window of missed messages.

Two modes:
1. **Manual restart** (default): User runs `temm1e update apply` or restarts the service
2. **Idle restart** (opt-in): If Perpetuum is in Sleep state AND no active sessions for >30 minutes, self-restart

Self-restart on Unix:
```rust
// 1. Replace binary: rename staged binary over current
// 2. exec() the new binary with same args
// The process replaces itself — PID stays the same, socket stays open
```

Self-restart on Windows:
```rust
// 1. Cannot replace running binary
// 2. Rename current to .bak
// 3. Copy staged to original path
// 4. Schedule restart via Windows Service Manager
// 5. Exit gracefully
```

---

## Security Risks — CRITICAL

### Risk 1: Compromised GitHub Release (CRITICAL)

**Threat:** Attacker gains access to the GitHub repo or CI pipeline and pushes a malicious release. All auto-updating clients install it.

**Real precedent:** Notepad++ supply chain attack (CVE-2025-15556, June-October 2025). State-sponsored attackers hijacked the update channel at the hosting level, delivering trojanized installers to targeted government organizations for 4 months.

**Mitigation:**
- SHA256 checksum verification (baseline — attacker who compromises the release can also compromise the checksum)
- **Ed25519 signature verification** — sign releases with a key NOT stored in CI. The signing key lives on a hardware token or air-gapped machine. The verify key is compiled into the binary. Even if CI is fully compromised, the attacker cannot produce a valid signature.
- Monotonic version enforcement — client refuses to install versions older than current (prevents rollback attacks)

**Residual risk:** If the signing key itself is compromised (theft, coercion), all verification is worthless. This is the same trust model as all code signing systems.

### Risk 2: Broken Update Bricks the Daemon (HIGH)

**Threat:** New release has a bug that prevents startup. The old binary is gone. The daemon is dead.

**Mitigation:**
- Keep the previous binary as `~/.temm1e/updates/temm1e-{prev_version}.bak`
- After updating, health-check the new binary: `temm1e --version` must succeed
- If health-check fails, automatically roll back to `.bak`
- Max 1 auto-rollback — if rollback also fails, stop and notify user

### Risk 3: MITM on Update Check (MEDIUM)

**Threat:** Attacker on the network suppresses update notifications, keeping users on vulnerable versions.

**Mitigation:** GitHub API is HTTPS. Certificate validation is handled by `reqwest` (which uses `rustls` or native TLS). No additional pinning needed — GitHub's certificate infrastructure is robust.

### Risk 4: Metered Connection / Bandwidth (LOW)

**Threat:** User is on a metered connection. Auto-downloading a 30MB binary is unwanted.

**Mitigation:** Config flag: `[updates] auto_download = false`. Default: true. When false, only check + notify, don't download.

### Risk 5: Supply Chain via Dependencies (LOW for update mechanism)

**Threat:** XZ Utils style — compromised dependency in the build chain.

**Mitigation:** This is a build-time concern, not an update-mechanism concern. The update system downloads a pre-built binary — it doesn't run `cargo build`. Verification is at the binary level (signatures), not the source level.

---

## The XZ Utils Lesson

The XZ backdoor (CVE-2024-3094) was injected into release tarballs, not the git repository. The malicious code was present in 35 Docker Hub images as late as August 2025. Key lesson: **release artifacts and git source can diverge.**

For TEMM1E:
- CI builds are reproducible (Cargo.lock pins all dependencies)
- Release binaries are built by GitHub Actions (transparent build logs)
- Ed25519 signatures tie the binary to a specific signing ceremony
- Users who build from source (`temm1e update` via git) are independently verifiable

The auto-update mechanism downloads release binaries, which means it trusts the CI pipeline. Ed25519 signing adds a second trust factor beyond CI.

---

## Auto-Update + Auto-Issue Integration

When the bug reporter files an issue, it includes the TEMM1E version:

```markdown
## [BUG] panic in context.rs
**Auto-reported by Tem v4.1.2**
```

When a new release fixes that issue:

```
Release v4.1.3 notes:
- fix(context): safe UTF-8 truncation (fixes #42)
```

The update checker parses the release body for `fixes #N` patterns. If any match issues that THIS installation reported, the update notification is elevated:

```
Update v4.1.3 available — fixes a bug you reported (#42).
Restart to apply: temm1e update apply
```

This closes the loop: Tem reports the bug, developers fix it, Tem tells the user "your bug is fixed, update when ready."

---

## Implementation Phases

| Phase | What | Risk | LOC |
|---|---|---|---|
| 1 | Version check (GitHub Releases API, compare semver) | Zero | ~40 |
| 2 | Perpetuum concern for periodic check during Idle | Zero | ~30 |
| 3 | Binary download + SHA256 verify + staging | Low | ~80 |
| 4 | `temm1e update apply` command (manual restart) | Low | ~50 |
| 5 | Ed25519 signature verification | Low | ~60 |
| 6 | Idle self-restart (opt-in) | Medium | ~40 |
| 7 | Auto-rollback on broken update | Low | ~40 |
| 8 | Issue-fix correlation (parse release notes) | Zero | ~30 |
| **Total** | | | **~370 LOC** |

---

## What NOT To Do

**Do NOT auto-restart without explicit user opt-in.** TEMM1E is an always-running daemon. Unexpected restarts drop active conversations. The default must be check + notify + stage, never auto-apply.

**Do NOT skip signature verification.** SHA256 checksums alone are insufficient — if the release is compromised, the checksum is too. Ed25519 signatures with an offline signing key are the minimum bar.

**Do NOT update during active sessions.** The update concern must check `conscience.state()` — only proceed when Idle or Sleep, never Active.

**Do NOT ship without rollback.** A broken update on a user's production server with no rollback is catastrophic. Keep the previous binary and auto-revert on health-check failure.

---

## Config

```toml
[updates]
auto_check = true       # Check for updates periodically (default: true)
auto_download = true    # Download updates in background (default: true)
auto_restart = false    # Self-restart during idle (default: false — opt-in)
channel = "stable"      # "stable" or "pre-release" (default: stable)
```

---

## Recommendation

**Build Phase 1-4 alongside the bug reporter.** Version checking and manual update are low-risk and immediately useful. Ed25519 signing (Phase 5) should be added before any auto-download goes live. Idle self-restart (Phase 6) is opt-in and can ship later.

The closed feedback loop (auto-issue → fix → auto-update notification) is the real differentiator. An AI agent that reports its own bugs and tells the user when the fix is available is qualitatively different from a dumb update checker.
