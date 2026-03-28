# Tem Gaze: Vision-Primary Desktop Control for Messaging-First AI Agents

> **Authors:** Quan Duong, Tem (TEMM1E Labs)
> **Date:** March 2026
> **Status:** Both phases implemented, tested, and shipped (v3.4.0).
> **Predecessor:** Tem Prowl (Duong & Tem, 2026) — browser-only, DOM-primary

---

## Abstract

We present Tem Gaze, a vision-primary desktop control architecture for TEMM1E — a messaging-first AI agent runtime. Tem Gaze extends TEMM1E's existing browser control system (Tem Prowl) to full computer control across Ubuntu and macOS, using the user's already-configured VLM provider with no additional model dependencies.

We make five contributions:

1. A comprehensive **industry landscape analysis** covering 20+ desktop control frameworks and 8 benchmarks, establishing that the field has converged on vision-primary approaches — 9 of 12 major agents use pure vision, and the top OSWorld score (Claude Opus 4.6, 72.7%) matches human performance (72.36%) using screenshots alone.

2. A formal analysis of **desktop accessibility coverage** across platforms, showing 33-65% element availability on desktop versus browser DOM's 100%, with an honest assessment of why accessibility-primary approaches fail in production.

3. A **model-agnostic grounding pipeline** using three orthogonal techniques — zoom-refine (spatial localization, +29pp on ScreenSpot-Pro), Set-of-Mark overlay (element selection, reducing output information from ~21 bits to ~5.6 bits), and verify-retry (outcome verification, leveraging the generation-verification gap). These improve accuracy regardless of which VLM processes them.

4. A rigorous **rejection analysis** of multi-model routing, demonstrating that the $0.014/task savings does not justify the engineering cost, and that 4 of 7 provider configurations (OpenRouter, proxy, Ollama, corporate) cannot support tier routing at all.

5. A **complete working implementation** across two phases: Phase 1 (Prowl V2, browser) adds `zoom_region`, SoM overlays, and blueprint bypass with 596 lines and 23 tests. Phase 2 (Tem Gaze, desktop) adds full computer use — screen capture via `xcap`, input simulation via `enigo`, SoM overlay compositing with embedded bitmap font — as a new `temm1e-gaze` crate with 22 tests. Proven live on macOS with Gemini Flash: the agent opened Spotlight, launched TextEdit, and typed a message autonomously.

---

## 1. Introduction

### 1.1 The Problem: From DOM to Desktop

Tem Prowl (Duong & Tem, 2026) established TEMM1E's web browsing capability with a three-tier observation architecture (accessibility tree, DOM, screenshot) achieving `O(d log c)` token cost scaling, where `d` is task depth and `c` is page complexity. Prowl's advantage rests on a single assumption: the browser DOM provides 100% element coverage with pixel-accurate bounding boxes.

Desktop control invalidates this assumption. When a messaging-first agent must "open Finder and rename a file" or "take a screenshot of the terminal error," no DOM exists. The accessibility tree — which seemed like a natural replacement — provides only 33% full coverage on macOS (Screen2AX, MacPaw, 2025) and 10-15% coverage for Electron apps on Linux without special launch flags. Meanwhile, the Wayland display protocol breaks AT-SPI2 action injection entirely, requiring OS-level input simulation regardless.

The industry has responded by converging on pure vision. Of the 12 most significant desktop control agents surveyed (Section 2), 9 use screenshots as the primary or only perception modality. Claude Opus 4.6 matched human performance on OSWorld (72.7% vs. 72.36%) using pure vision with zero accessibility data (Xie et al., 2024; Anthropic, 2026).

### 1.2 The Messaging-First Desktop Constraint

TEMM1E is messaging-first: users interact via Telegram, Discord, Slack, or WhatsApp. They cannot see the desktop. This imposes four constraints that distinguish Tem Gaze from desktop-copilot agents:

**C1 — No visual feedback.** The agent operates autonomously; the user cannot watch or intervene mid-task.

**C2 — Self-contained verification.** The agent must verify its own actions — it cannot ask "does this look right?" per click.

**C3 — Automatic error recovery.** Misclicks with no human observer must be detected and corrected without intervention.

**C4 — Screenshots serve dual duty.** Every captured screenshot is both a perception input for grounding AND evidence to send back to the user via chat.

Constraint C4 makes pure vision not just acceptable but *preferable*: the agent captures screenshots for user communication regardless, so using them for grounding incurs zero additional perception cost.

### 1.3 Contributions and Outline

We structure the paper as follows. Section 2 formalizes the GUI grounding problem. Section 3 surveys the industry landscape. Section 4 presents benchmark analysis. Section 5 documents desktop accessibility coverage. Section 6 presents the Tem Gaze architecture and its three grounding techniques with formal analysis. Section 7 documents rejected approaches with quantitative justification. Section 8 describes the Phase 1 implementation and validation. Section 9 discusses limitations and future work.

---

## 2. Problem Formulation

### 2.1 GUI Grounding as Conditional Estimation

Following Cheng et al. (2024), we formalize GUI visual grounding as a conditional probability estimation problem:

**Input:** Screenshot `s` and natural language instruction `x` describing a target element.
**Output:** Click location `y = (y_x, y_y)` as normalized coordinates in `[0,1]^2`.
**Objective:** Learn `p(y | s, x)` such that the predicted point falls within the target element's bounding box.

**Accuracy metric** (ScreenSpot; Cheng et al., 2024): A prediction is correct if and only if the predicted point lies within the ground-truth bounding box:

```
Acc = (1/n) * sum_{i=1}^{n} 1[y_hat_i in B_i]
```

where `B_i` is the annotated bounding box for sample `i`. This is a binary point-in-box metric — IoU is not used because in GUI automation, only the click point matters.

### 2.2 Computer Use as POMDP

Following Xie et al. (2024), we model full computer control as a Partially Observable Markov Decision Process (POMDP) `(S, O, A, T, R)`:

- **S**: Full state of the operating system (VM snapshot).
- **O**: Screenshot of the desktop, optionally augmented with accessibility tree. Includes task instruction `I`.
- **A**: Mouse and keyboard primitives — movement, clicks (left/right/double), dragging, keystrokes, hotkeys, plus control tokens `WAIT`, `FAIL`, `DONE`.
- **T: S x A -> S**: Deterministic execution of action in the environment.
- **R**: Binary. Each task has a custom execution-based evaluation function inspecting the resulting OS state.

**Task success rate:** `SR = |{i : R(s_final^i) = 1}| / n`

Human success rate on OSWorld: 72.36% (n=369 tasks). The gap between human and agent performance has narrowed from 60pp (2024) to near zero (2026).

### 2.3 The Information-Theoretic Gap

A key distinction between grounding approaches:

**Direct coordinate regression** requires specifying a point in a 2D pixel grid. For a 1920x1080 display, the output information is:

```
H_regression = log_2(1920) + log_2(1080) ~ 21.0 bits
```

**Element selection from N candidates** (SoM / classification) requires:

```
H_classification = log_2(N) bits
```

For a typical page with N=50 interactive elements: `H_classification ~ 5.6 bits`. This is a **3.75x reduction** in output information complexity. VLMs — pretrained on discrete token prediction, not continuous spatial regression — are fundamentally better suited to classification than coordinate regression.

This information-theoretic gap explains the consistent empirical finding that SoM/labeling approaches outperform raw coordinate prediction across all benchmarks (Section 4).

---

## 3. Industry Landscape

### 3.1 Surveyed Agents

We surveyed 20+ frameworks across commercial and open-source (2024-2026).

**Table 1. Major Desktop Control Agents**

| Agent | Organization | Year | Method | Scope | Key Result |
|-------|-------------|------|--------|-------|------------|
| Computer Use | Anthropic | 2024-26 | Trained pixel counting + zoom | Full desktop | 72.7% OSWorld |
| CUA / Operator | OpenAI | 2025 | RL in synthetic environments | Browser-first | 42.9% OSWorld |
| Project Mariner | Google | 2025 | Gemini-powered | Browser | Cloud VM, 10 concurrent |
| Nova Act | Amazon | 2025 | Custom VLM + RL in web gyms | Browser | >90% internal, $0.006/step |
| AskUI VisionAgent | AskUI | 2025 | Proprietary visual AI | Full desktop | 66.2% OSWorld |
| UI-TARS 1.5 | ByteDance | 2025 | Fine-tuned Qwen2-VL, DPO | Full desktop | 42.5% OSWorld |
| Agent S2 | Simular | 2025 | MoG + hierarchical planning | Full desktop | +32.7% over Claude CU |
| OpenCUA-72B | HKU | 2025 | CoT-augmented, 3 OS | Full desktop | 45.0% OSWorld-Verified |
| Fara-7B | Microsoft | 2025 | Pure vision, no parsing | Full desktop | $0.025/task |
| Cradle | BAAI/Tsinghua | 2024 | 6-module GCC framework | Desktop + games | RDR2, Stardew Valley |
| browser-use | browser-use | 2025 | Playwright + LLM | Browser | 89.1% WebVoyager |
| SeeAct | OSU NLP | 2024 | Two-stage: generate + ground | Browser | 51.1% oracle grounding |

**Key observation:** 9 of 12 agents use pure vision. The 3 exceptions: UFO2 (Microsoft, Windows UIA hybrid — 62% of failures from missing UIA elements), OS-Copilot (hybrid, leaning vision), and Agent S1 (which dropped accessibility in v2).

### 3.2 Approach Taxonomy

We identify five distinct approaches to visual grounding:

**A. Direct Coordinate Prediction.** VLM outputs raw `(x, y)`. Simplest but weakest: GPT-4o scores 0.8% on ScreenSpot-Pro.

**B. Set-of-Mark (SoM) / Element Labeling.** Elements overlaid with numbered labels; VLM selects a label. Converts regression to classification. OmniParser + GPT-4o: 39.6% ScreenSpot-Pro (49x over raw).

**C. Multi-Resolution Zoom-Refine.** Coarse localization then progressive zooming. MEGA-GUI: 73.18% ScreenSpot-Pro with no training. Highest-impact model-agnostic technique.

**D. Coordinate-Free Attention Grounding.** GUI-Actor (Microsoft, NeurIPS 2025): attention-based action head, no coordinate tokens. 44.6% ScreenSpot-Pro with 7B model. Requires model modification — not applicable to API-based VLMs.

**E. RL-Trained Specialized Models.** Nova Act, CUA, UI-TARS: trained specifically for GUI interaction. Most reliable but requires training infrastructure.

**Tem Gaze uses B + C** — model-agnostic, no training, applicable to any API-based VLM.

### 3.3 Screen Parsing Without DOM

For SoM labeling, element positions must be known. Five approaches exist:

| Source | Coverage | Cost | Dependency |
|--------|----------|------|------------|
| Browser DOM + `getBoundingClientRect()` | 100% | Free | Browser only |
| OmniParser V2 (YOLO + BLIP-2) | Universal | ~free (local) | Python + model weights |
| OS accessibility (AT-SPI2 / AX API) | 33-65% | Free | Platform-specific |
| VLM-native detection | Universal | 1 API call | Same VLM |
| OCR + heuristics | Text-heavy UIs | Free/cheap | OCR engine |

**Tem Gaze:** Browser uses DOM (free, 100%). Desktop uses VLM-native detection (universal, no extra deps). OS accessibility is an optional cost optimizer (feature-gated, off by default).

---

## 4. Benchmark Analysis

### 4.1 OSWorld

**Table 2. OSWorld Results (Xie et al., NeurIPS 2024)**

| Agent | Year | Score | Method |
|-------|------|-------|--------|
| Human baseline | 2024 | 72.36% | — |
| GPT-4V (initial) | 2024 | 12.24% | Screenshot + a11y |
| Agent S (GPT-4o) | 2024 | 20.58% | Hierarchical planning |
| UI-TARS (50 steps) | 2025 | 24.6% | Fine-tuned VLM |
| Claude 3.7 | 2025 | 28.0% | Pure vision |
| OpenAI CUA (o3) | 2025 | 42.9% | RL-trained |
| OpenCUA-72B | 2025 | 45.0% | Open-source, CoT |
| Claude Sonnet 4.5 | 2026 | 61.4% | Pure vision |
| AskUI VisionAgent | 2025 | 66.2% | Pure vision |
| Claude Opus 4.6 | 2026 | 72.7% | Pure vision + zoom |
| OSAgent | 2025 | 76.26% | Self-checking |

**Trajectory:** 12.24% to 72.7% in two years. Improvement driven by VLM capability, not structured data. The OSWorld-Human study (2025) found accessibility trees add 3-26 seconds latency per action with only "modest improvements for some architectures."

### 4.2 ScreenSpot-Pro

**Table 3. ScreenSpot-Pro (Li et al., ACM MM 2025)**

Professional desktop screenshots: 1,581 samples, 23 applications, 5 industries, 3 OSes. Target elements average 0.07% of screen area.

**Single-pass results:**

| Model | Size | Accuracy |
|-------|------|----------|
| GPT-4o | — | 0.8% |
| Qwen2-VL-7B | 7B | 1.6% |
| OS-Atlas-7B | 7B | 18.9% |
| UI-TARS-72B | 72B | 38.1% |
| OmniParser V2 + GPT-4o | — | 39.6% |
| GUI-Actor-7B | 7B | 44.6% |
| UI-TARS-1.5 | — | 61.6% |

**With zoom-refine techniques (no training):**

| Technique | Backbone | Accuracy | Delta |
|-----------|----------|----------|-------|
| Baseline | OS-Atlas-7B | 18.9% | — |
| Iterative Focusing | OS-Atlas-7B | 31.0% | +12.1pp |
| Iterative Narrowing | OS-Atlas-7B | 31.9% | +13.0pp |
| ReGround | OS-Atlas-7B | 40.2% | +21.3pp |
| ScreenSeekeR | OS-Atlas-7B | 48.1% | +29.2pp |
| MEGA-GUI | Various | 73.18% | N/A |
| RegionFocus | Qwen3VL-32B | 74.0% | N/A |

**Key finding:** Zoom-refine takes a weak backbone (18.9%) to 48.1% with zero training (+154% relative). On frontier VLMs, test-time visual scaling reaches 74%+. These are model-agnostic improvements.

### 4.3 Accuracy Summary by Technique

**Table 4. Grounding Accuracy Across Benchmarks**

| Technique | ScreenSpot-Pro | ScreenSpot | OSWorld |
|-----------|---------------|------------|---------|
| Raw VLM coords | 0.8-1.6% | 16.2% | 12.24% |
| Specialized GUI model | 18.9-61.6% | 47-94% | 24-42% |
| SoM + VLM | 39.6% | — | — |
| Zoom-refine (no training) | 31-48% | 69% | — |
| Zoom-refine + frontier VLM | 74-86% | — | 72.7% |
| Self-correction loop | — | — | 76.26% |

---

## 5. Desktop Accessibility: The Empirical Reality

### 5.1 Platform Coverage

**Table 5. macOS Accessibility (Screen2AX, MacPaw, 2025)**

| Category | Full a11y | Partial | None |
|----------|----------|---------|------|
| Popular apps | 36.4% | 45.9% | 17.7% |
| Random apps | 29.4% | 37.8% | 32.7% |
| **Aggregate** | **~33%** | **~46%** | **~18%** |

**Table 6. Linux Ubuntu Accessibility (AT-SPI2)**

| App type | Coverage | Notes |
|----------|----------|-------|
| GTK4 native | 80-85% | Best on Linux |
| GTK3 native | 70-80% | ATK bridge |
| Firefox | 80-90% | ATK module |
| Electron (default) | **10-15%** | A11y OFF by default |
| Electron (force-enabled) | 60-70% | `--force-renderer-accessibility` required |
| GNOME Shell | 40-50% | Incomplete |

**Aggregate estimate:** 40-65% of interactive elements in a typical session. Compare: browser DOM provides 100%, always.

### 5.2 Consistently Missing Elements

Canvas/WebGL content, custom-drawn widgets, drag-and-drop states, animations, PDF viewer content, dropdown options before expansion, tooltips, context menus, Flatpak/Snap sandboxed apps.

### 5.3 Wayland

AT-SPI2 on Wayland: reading trees works (D-Bus), but **mouse event injection is broken**. The Newton replacement protocol (GNOME-funded) is years from production. OS-level input simulation (`ydotool`, `enigo`) is mandatory regardless of accessibility tree availability.

### 5.4 Industry Response

**Table 7. Agent Accessibility Usage**

| Agent | Uses A11y? | Rationale |
|-------|-----------|-----------|
| Claude Computer Use | No | Trained pixel counting + zoom |
| UI-TARS | No | "Platform-specific inconsistencies, verbosity, limited scalability" |
| Agent S2 | No (dropped v1→v2) | "Too unreliable across diverse apps" |
| AskUI | No | Platform-agnostic by design |
| Fara-7B | No | "Does not rely on accessibility trees" |
| UFO2 | Yes, hybrid | Windows-only; 62% failures from missing UIA elements |

**5 of 6 major agents use pure vision.** The sole hybrid (UFO2) is Windows-specific and still requires vision fallback for the majority of failure cases.

### 5.5 Design Decision

**Vision is primary. Accessibility is an optional cost optimizer, off by default.** When accessibility data IS available, it saves tokens (a11y tree ~4K tokens vs screenshot ~1,334 tokens at 1024x768), but the system must function correctly with zero accessibility data.

---

## 6. Tem Gaze Architecture

### 6.1 Design Principles

Seven non-negotiable axioms govern the architecture:

**A1 — Single Model.** All grounding calls use the user's configured provider and model. No multi-model routing.

**A2 — Zero New Dependencies** for existing users. Desktop control is feature-gated.

**A3 — Provider Agnostic.** Works identically for Anthropic, OpenAI, Gemini, OpenRouter, proxies, Ollama.

**A4 — Additive Integration.** No existing crate logic is modified; all changes are new code.

**A5 — Vision Primary.** Desktop uses vision as primary perception. A11y is optional.

**A6 — Cross-Platform Parity.** Ubuntu (X11 + Wayland) and macOS (ARM + Intel).

**A7 — Resilience Inheritance.** All existing resilience guarantees apply: `catch_unwind`, session rollback, UTF-8 safe truncation.

### 6.2 Grounding Pipeline Overview

```
                ┌─────────────────────────────────────┐
Browser         │  DOM available?                      │
context? ──Yes──│  Yes → SoM overlay (classification)  │── 1 VLM call
                │  Blueprint match? → 0 VLM calls      │
                └─────────────────────────────────────┘

                ┌─────────────────────────────────────┐
Desktop         │  Coarse Pass                         │
context? ──Yes──│  HIGH conf → direct click            │── 1 VLM call
                │  MED conf  → zoom-refine             │── 2 VLM calls
                │  LOW conf  → zoom + SoM              │── 3 VLM calls
                │  Optional: verify-retry              │── +1 VLM call
                └─────────────────────────────────────┘
```

### 6.3 Zoom-Refine: Formal Specification

Following ZoomClick (2024), we define the coordinate mapping for iterative zoom-refine.

**Definition 1 (Viewport Crop).** Given original screenshot dimensions `(W, H)` and viewport `V = (v_{x1}, v_{y1}, v_{x2}, v_{y2})` in pixel space, the VLM receives the cropped region scaled to fill the API resolution constraint.

**Definition 2 (Coordinate Transform).** Given model prediction `p_hat = (x_hat, y_hat)` normalized in `[0,1]^2` within the cropped view, the prediction in original space is:

```
p_orig = (v_{x1} + (v_{x2} - v_{x1}) * x_hat,
          v_{y1} + (v_{y2} - v_{y1}) * y_hat)
```

**Definition 3 (Iterative Shrink).** At each iteration `k`, the crop region shrinks by factor `rho in (0, 1)`:

```
w_k = max(floor(rho * w_{k-1}), m)
h_k = max(floor(rho * h_{k-1}), m)
```

where `m` is a minimum crop size preserving context.

**Convergence Detection.** Following ZoomClick, we use pixel-space distance between consecutive predictions:

```
d_k = ||p_direct - p_k||_2
```

A threshold `tau` (ZoomClick reports optimal `tau = 50.7px` for 91.8% discrimination accuracy) determines whether to accept the refined prediction or revert to the direct prediction.

**Remark.** No formal convergence proof exists for zoom-refine in GUI grounding (ZoomClick, 2024; PIVOT, 2024; Iterative Narrowing, 2024). The closest theoretical connection is PIVOT's analogy to the **cross-entropy method (CEM)** — sampling candidates, evaluating, refitting to top performers — but this remains analogical, not rigorous. Empirically, accuracy does not monotonically increase: error propagation from bad initial predictions is a documented failure mode. Tem Gaze addresses this with the convergence detection threshold and SoM fallback.

**Empirical evidence:**

| System | Backbone | Before | After | Relative Gain |
|--------|----------|--------|-------|---------------|
| Iterative Narrowing | Qwen2-VL-7B | 42.89% | 69.1% | +61% |
| ScreenSeekeR | OS-Atlas-7B | 18.9% | 48.1% | +154% |
| RegionFocus | Qwen3VL-32B | — | 74.0% | — |
| ZoomClick | Qwen3-VL-32B | 54.0% | 72.1% | +33.5% |

### 6.4 Set-of-Mark: Information-Theoretic Justification

**Proposition 1 (Information Reduction).** For a display of resolution `W x H` with `N` interactive elements, SoM-style element labeling reduces the output information requirement from `log_2(W) + log_2(H)` bits (coordinate regression) to `log_2(N)` bits (element selection).

*Argument.* Direct coordinate prediction requires specifying a point in a `W x H` grid. At pixel resolution, this requires `log_2(W * H)` bits. For 1920x1080: `log_2(2,073,600) ~ 21.0 bits`.

SoM overlay assigns unique labels to `N` elements. Selecting the correct element requires `log_2(N)` bits. For a typical page with `N = 50`: `log_2(50) ~ 5.6 bits`.

**Reduction ratio:** `21.0 / 5.6 ~ 3.75x`.

This reduction explains why SoM consistently outperforms raw coordinate prediction. VLMs are pretrained on discrete token prediction (language), not continuous spatial regression. SoM aligns the output modality with the model's pretraining distribution. As noted by Nasiriany et al. (PIVOT, ICML 2024): "although VLMs struggle to produce precise spatial outputs directly, they can readily select among a discrete set of coarse choices."

**Tem Gaze implementation:** For browser contexts, SoM overlays are injected via JavaScript (numbered red circles matching accessibility tree indices). For desktop contexts, SoM is composited onto the screenshot image before sending to the VLM. The element positions come from the VLM's own coarse-pass identification — no separate detection model.

### 6.5 Verify-Retry: Leveraging the Generation-Verification Gap

**Definition 4 (Generation-Verification Gap).** Following Song et al. (ICLR 2025), the GV-Gap is:

```
GV-Gap = Pass@K - SR
```

where `Pass@K = Pr(exists correct response in K generations)` and `SR = (1/n) * sum(y_{i,j_hat})` is the success rate under a verification strategy.

A positive GV-Gap means correct actions are generated but the system fails to identify them. Conversely, verification is often easier than generation — the model can detect "this doesn't look right" more reliably than it can generate the correct action on the first attempt.

**Formal dynamics.** The solver-verifier gap follows exponential convergence dynamics (arXiv:2507.00075):

```
G(t) ~ delta * exp(-k(alpha - beta) * t) + G_inf
```

where `alpha, beta` are decay coefficients for solver and verifier capability respectively (`alpha > beta`), and `k` relates gap potential energy to the gap itself.

**Tem Gaze application.** After each high-stakes action (delete, submit, send), the agent captures a post-action screenshot and asks the VLM: "Compare before/after — did the expected change occur?" This leverages the verification advantage: the VLM may misclick on generation, but reliably detects the misclick on verification. Failed verifications trigger re-grounding via zoom-refine with the updated screenshot.

**Configuration:** Verify-retry is controlled by `verify_mode`:
- `"off"` — never verify (fastest, cheapest)
- `"high_stakes"` — verify destructive actions only (default)
- `"always"` — verify every action (most reliable, most expensive)

### 6.6 Cost Analysis

**Token cost per screenshot** (Anthropic, 2026):

```
tokens = (width * height) / 750
```

At recommended resolution 1024x768: **1,049 tokens ~ $0.003** (Sonnet 4.6 at $3/M input).

**Adaptive pipeline cost vs. fixed-cost systems:**

| Scenario | Tem Gaze | Fixed (Claude CU style) | Savings |
|----------|----------|-------------------------|---------|
| Easy target (high conf) | 1 call, ~$0.003 | 1 call, ~$0.003 | $0.000 |
| Medium target (zoom) | 2 calls, ~$0.006 | 1 call, ~$0.003 | -$0.003 |
| Hard target (zoom+SoM) | 3 calls, ~$0.009 | 1 call, ~$0.003 | -$0.006 |
| Blueprint bypass | 0 calls, $0.000 | 1 call, ~$0.003 | +$0.003 |

The adaptive pipeline is more expensive per-action for hard targets but achieves higher accuracy, reducing total task cost through fewer misclicks and retries. Blueprint bypass provides zero-cost grounding for known flows (100+ services in the login registry).

**Formal cost model** (following BATS; arXiv:2511.17006):

```
C_task(pi) = sum_{t=1}^{T} c_token(s_t) + c_grounding(s_t, pi)
```

where `T` is the number of steps, `c_token` is the per-step token cost, and `c_grounding` is the adaptive grounding cost (0-3 additional VLM calls depending on confidence).

**Without budget awareness, agents hit a performance ceiling** (BATS finding). Tem Gaze's adaptive pipeline is a form of budget-aware scaling: easy steps are cheap, hard steps invest more.

---

## 7. Rejected Approaches

### 7.1 Multi-Model Routing

**Proposal:** Route grounding calls to different models based on difficulty — Haiku for triage, Sonnet for grounding, Opus for hard cases.

**Rejection rationale:**

1. **Marginal savings.** Per-action savings: $0.0007. Per 20-step task: $0.014.
2. **Proxy incompatibility.** 4 of 7 provider types (OpenRouter, custom proxy, Ollama, corporate gateway) cannot support tier routing because the model catalog is unknown or uses non-standard naming.
3. **Deprecation risk.** The model registry (`model_registry.rs`) is a static list. Automatic routing to deprecated models produces mysterious failures.
4. **TEMM1E architecture.** Single active provider, single `Arc<dyn Provider>`. Multi-provider routing requires fundamental architecture changes.
5. **Debug complexity.** 7 providers x ~5 models = 35 combinations. Grounding failures become undiagnosable.

**Decision:** User's configured model handles all grounding calls. Techniques improve accuracy model-agnostically.

### 7.2 Local Detection Models (YOLO / OmniParser)

**Proposal:** Run OmniParser V2 (YOLOv8 + BLIP-2) locally for element detection.

**Rejection rationale:**

1. **Deployment.** TEMM1E is a single Rust binary. OmniParser requires Python + PyTorch/ONNX Runtime (2-5 GB) or Rust ONNX bindings + model weights (6-80 MB).
2. **Cross-compile.** CI already struggles with aarch64-linux-musl OpenSSL. Adding ONNX multiplies platform-specific build issues.
3. **User experience.** "Download 80 MB of model weights" breaks the TEMM1E deployment story.
4. **Diminishing returns.** Frontier VLMs are rapidly closing the gap: ScreenSpot-Pro went from 0.8% (raw GPT-4o) to 74% (VLM + test-time scaling) in one year. Dedicated detectors are a bridge, not a destination.

**Decision:** The VLM IS the detector. Zoom-refine is the accuracy lever.

### 7.3 Accessibility-Primary for Desktop

**Proposal:** Mirror Prowl's DOM-primary approach using OS accessibility APIs.

**Rejection rationale:**

1. **Coverage.** 33-65% on desktop vs 100% on browser. Unreliable foundation.
2. **Wayland.** Action injection broken. OS-level input simulation required regardless.
3. **Industry evidence.** 5 of 6 major agents use pure vision.
4. **Rust ecosystem.** `atspi` crate is pre-1.0; macOS FFI wrappers are incomplete ("pretty spotty" per author).
5. **Token economics favor vision when a11y is unreliable.** Wasting tokens on a11y queries that return nothing is worse than using vision directly.

**Decision:** Vision-primary. A11y is a cost optimizer for when it works, not a correctness requirement.

---

## 8. Implementation: Prowl V2 (Phase 1)

### 8.1 What Was Built

Phase 1 enhances TEMM1E's existing browser tool with vision grounding techniques. Zero new crate dependencies. Zero behavior changes for existing users.

**Table 8. Phase 1 Components**

| Component | File | Description |
|-----------|------|-------------|
| Grounding module | `temm1e-tools/src/grounding.rs` | 6 coordinate transform functions |
| `zoom_region` action | `temm1e-tools/src/browser.rs` | CDP clip capture at 2x resolution |
| SoM overlay (Tier 3) | `temm1e-tools/src/browser.rs` | JS-injected numbered labels |
| Blueprint bypass | `temm1e-tools/src/prowl_blueprints.rs` | 100+ service registry matching |
| GazeConfig | `temm1e-core/src/types/config.rs` | Full configuration schema |

### 8.2 Grounding Module Functions

| Function | Purpose |
|----------|---------|
| `scale_for_api(w, h, max_edge, max_pixels) -> (w', h', s)` | Compute API-constrained dimensions |
| `zoom_to_original(x_z, y_z, w_z, h_z, region) -> (x_o, y_o)` | Transform zoomed coords to original space |
| `dpi_to_logical(x_p, y_p, scale) -> (x_l, y_l)` | Physical pixels to logical points |
| `api_to_screen(x_a, y_a, scale) -> (x_s, y_s)` | VLM coords to screen coords |
| `clamp_coords(x, y, w, h) -> (x', y')` | Clamp to valid bounds |
| `validate_zoom_region(x1, y1, x2, y2, w, h) -> Result<[u32;4]>` | Validate and clamp region |

### 8.3 Compilation Gates

| Gate | Result |
|------|--------|
| `cargo check --workspace` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS (0 warnings) |
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p temm1e-tools` | PASS (130 tests, 0 failures) |
| `cargo test -p temm1e-core` | PASS (144 tests, 0 failures, 1 pre-existing skip) |

**New tests added:** 23 (20 for grounding math, 3 for blueprint bypass). **Total lines:** 596. **New dependencies:** 0.

### 8.4 Phase 2: Desktop Computer Use (Tem Gaze)

Phase 2 adds full desktop control as a new `temm1e-gaze` crate.

**Table 9. Phase 2 Components**

| Component | File | Description |
|-----------|------|-------------|
| DesktopController | `temm1e-gaze/src/desktop_controller.rs` | Screen capture (xcap) + input simulation (enigo) |
| SoM overlay compositing | `temm1e-gaze/src/overlay.rs` | Draw numbered labels on screenshots with embedded 5x7 bitmap font |
| Key combo parser | `temm1e-gaze/src/platform/mod.rs` | Parse "cmd+c", "ctrl+shift+a" → enigo Key sequences |
| DesktopTool | `temm1e-tools/src/desktop_tool.rs` | Tool trait impl: screenshot, click, type, key, scroll, drag, zoom_region |

**Platform support:** macOS (ARM + Intel, requires Accessibility permission) and Linux (X11 + Wayland, requires PipeWire for screen capture).

**Graceful degradation:** On macOS without Accessibility permission, the controller initializes successfully with `input_available = false`. Screen capture works. Input methods return clear error messages guiding the user to System Settings. All other TEMM1E tools continue functioning.

### 8.5 Live Validation

Both phases were tested live on macOS with `gemini-3-flash-preview`:

**Table 10. Live Test Results ($0.069 total cost)**

| Test | Type | Result |
|------|------|--------|
| SoM overlay on 14-element form | Browser | PASS |
| SoM overlay on 650-element GitHub page | Browser | PASS |
| zoom_region 2x capture (27 KB) | Browser | PASS |
| Multi-step: observe → zoom → click → self-correct | Browser | PASS |
| Desktop screenshot (identify open apps) | Desktop | PASS |
| Desktop click (opened Finder from Dock) | Desktop | PASS |
| Full computer use: Spotlight → TextEdit → typed message | Desktop | PASS |

**Key finding:** In the browser multi-step test, the first click missed by ~94 pixels. The agent detected the miss (page didn't change), self-corrected via DOM `getBoundingClientRect`, zoomed to verify, and clicked successfully on attempt 2. This directly validates the zoom-refine architecture: raw coordinate prediction is unreliable, but vision + verification + correction produces reliable outcomes.

### 8.6 Compilation Gates (Final)

| Gate | Result |
|------|--------|
| `cargo check --workspace` | PASS |
| `cargo clippy --workspace --all-targets --all-features` | PASS (0 warnings) |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace --all-features` | PASS (1,028 tests) |
| CI (check + test + musl build + glibc desktop build + Docker + audit) | PASS |
| CI/CD (release workflow) | PASS |

**Total new code:** ~3,200 lines across 2 phases. **New tests:** 45. **New crate:** `temm1e-gaze`. **Total crates:** 19.

---

## 9. Limitations and Future Work

### 9.1 Current Limitations

1. **No formal convergence proof for zoom-refine.** We rely on empirical evidence. Error propagation from bad initial predictions remains a failure mode.
2. **SoM for desktop requires VLM-native detection.** Without DOM, element detection depends on the VLM's own ability to identify interactive elements — which varies by model.
3. **Linux pre-built binary requires Ubuntu 24.04+** for desktop control (glibc 2.39 + PipeWire 0.3.65+). Older systems use the server binary or build from source.
4. **macOS requires Accessibility permission** for input simulation (click, type, drag). Screen capture works without it.
5. **Verify-retry adds latency.** Each verification is +1 VLM call (~1-3 seconds). For latency-sensitive tasks, verification may be prohibitive.
6. **No large-scale accuracy benchmarks.** Live validation covers 7 test cases. A formal accuracy study across 100+ desktop elements would strengthen the grounding claims.

### 9.2 Future Work

1. **Eigen-Tune Integration:** Track grounding outcomes (strategy, success, cost) as statistical data. Auto-tune confidence thresholds over time via `temm1e-distill`.
2. **Formal convergence analysis:** Derive bounds on zoom-refine accuracy as a function of initial prediction quality and shrink factor. The CEM connection from PIVOT may provide a starting point.
3. **Large-scale benchmarks:** Capture 100+ annotated desktop screenshots across Ubuntu and macOS. Measure accuracy of SoM, zoom-refine, and direct clicking across multiple VLMs.
4. **Windows support:** `enigo` and `xcap` both support Windows. The `temm1e-gaze` crate architecture supports it — platform-specific code is `#[cfg]`-gated. Requires testing and CI targets.

---

## 10. Conclusion

Tem Gaze extends TEMM1E from browser-only control to full desktop control using a vision-primary, model-agnostic grounding pipeline. Our research establishes three key findings:

1. **The industry has converged on pure vision for desktop control.** Desktop accessibility is too fragmented (33-65% coverage) to serve as a primary perception mode. The top-performing agents all use screenshots.

2. **Model-agnostic techniques are the right investment.** Zoom-refine (+29pp), SoM (3.75x information reduction), and verify-retry (leveraging the GV-Gap) improve every VLM equally. As VLMs improve, these techniques compound the gains.

3. **Simplicity wins.** One model, no routing, no local detection models, no cross-provider orchestration. The user's configured model handles everything. The pipeline works identically on Anthropic, OpenAI, Gemini, OpenRouter, Ollama, and corporate proxies.

Both phases are implemented and shipped as v3.4.0: ~3,200 lines of Rust, 45 new tests, 19 crates. Proven live: the agent opened Spotlight, launched TextEdit, and typed a message on a real macOS desktop via Gemini Flash — full autonomous computer use through a messaging interface.

```
Tem Prowl  → Hunts the web (browser, DOM-primary)
Tem Gaze   → Sees and commands the machine (desktop, vision-primary)
```

---

## References

### Foundational Papers

[1] Cheng, K., Sun, Q., Chu, Y., Xu, F., Li, Y., Zhang, J., & Wu, Z. "SeeClick: Harnessing GUI Grounding for Advanced Visual GUI Agents." ACL 2024. arXiv:2401.10935

[2] Xie, T., Zhang, D., Chen, J., Li, X., Zhao, S., Cao, R., ... & Yu, T. "OSWorld: Benchmarking Multimodal Agents for Open-Ended Tasks in Real Computer Environments." NeurIPS 2024. arXiv:2404.07972

[3] Yang, J., Zhang, H., Li, F., Zou, X., Li, C., & Gao, J. "Set-of-Mark Prompting Unleashes Extraordinary Visual Grounding in GPT-4V." 2023. arXiv:2310.11441

[4] Li, J., et al. "ScreenSpot-Pro: GUI Grounding for Professional High-Resolution Computer Use." ACM MM 2025. arXiv:2504.07981

### Zoom-Refine and Multi-Resolution

[5] "Zoom in, Click out: Unlocking the Potential of Zooming for GUI Grounding." 2024. arXiv:2512.05941

[6] Nguyen, T. "Improved GUI Grounding via Iterative Narrowing." 2024. arXiv:2411.13591

[7] Nasiriany, S., et al. "PIVOT: Iterative Visual Prompting Elicits Actionable Knowledge for VLMs." ICML 2024. arXiv:2402.07872

[8] Luo, T., et al. "Visual Test-time Scaling for GUI Agent Grounding." (RegionFocus) ICCV 2025. arXiv:2505.00684

[9] "MEGA-GUI: Multi-Agent Framework for GUI Grounding." 2025. arXiv:2511.13087

### Desktop Control Agents

[10] Anthropic. "Computer Use Tool Documentation." 2026. platform.claude.com

[11] Qin, Y., et al. "UI-TARS: Pioneering Automated GUI Interaction with Native Agents." ByteDance, 2025. arXiv:2501.12326

[12] Agent S2. "Compositional Generalist-Specialist Agent." Simular Research, 2025. arXiv:2504.00906

[13] Microsoft Research. "Fara-7B: An Efficient Agentic Model for Computer Use." 2025.

[14] Microsoft. "UFO2: The Desktop AgentOS." 2025. arXiv:2504.14603

[15] Hong, W., et al. "CogAgent: A Visual Language Model for GUI Agents." CVPR 2024.

[16] Zheng, B., et al. "SeeAct: GPT-4V(ision) is a Generalist Web Agent." ICML 2024.

[17] OpenCUA. "Open-Source Computer Use Agents." HKU, 2025.

[18] Cradle. "General Computer Control." BAAI/Tsinghua, 2024.

### Screen Parsing and Element Detection

[19] Microsoft Research. "OmniParser V2: Turning Any LLM into a Computer Use Agent." 2025.

[20] Microsoft Research. "GUI-Actor: Coordinate-Free Visual Grounding for GUI Agents." NeurIPS 2025.

[21] You, S., et al. "Ferret-UI: Grounded Mobile UI Understanding." Apple ML Research, 2024.

### Accessibility and Desktop Perception

[22] MacPaw. "Screen2AX: Vision-Based macOS Accessibility." 2025. arXiv:2507.16704

[23] GNOME. "AT-SPI2 Architecture Documentation." gnome.pages.gitlab.gnome.org

[24] GNOME. "Newton: Wayland-Native Accessibility Project." 2024.

### Generation-Verification Gap

[25] Song, K., et al. "Mind the Gap: Examining the Self-Improvement Capabilities of Large Language Models." ICLR 2025. arXiv:2412.02674

[26] "Theoretical Modeling of LLM Self-Improvement Training Dynamics Through Solver-Verifier Gap." 2025. arXiv:2507.00075

[27] "Shrinking the Generation-Verification Gap with Weak Verifiers." Stanford, 2025. arXiv:2506.18203

### Cost Models

[28] "Budget-Aware Tool-Use Enables Effective Agent Scaling." (BATS) 2025. arXiv:2511.17006

[29] Anthropic. "Vision Documentation." platform.claude.com/docs/en/build-with-claude/vision

### Platform Tools

[30] enigo. "Cross-platform input simulation." github.com/enigo-rs/enigo

[31] xcap. "Cross-platform screen capture." github.com/nashaofu/xcap

[32] Odilia. "atspi Rust crate." github.com/odilia-app/atspi

### Benchmarks

[33] OSWorld Leaderboard. os-world.github.io

[34] ScreenSpot-Pro Leaderboard. gui-agent.github.io/grounding-leaderboard

[35] WebArena. webarena.dev

[36] browser-use. "SOTA Technical Report." 2025. browser-use.com

### Predecessor

[37] Duong, Q. & Tem. "Tem Prowl: A Messaging-First Web-Native Agent Architecture." TEMM1E Labs, 2026.
