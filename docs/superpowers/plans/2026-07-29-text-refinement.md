# Text Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe, local, two-stage text refinement to streamed dictation without weakening the existing append-only injection path.

**Architecture:** Keep `stream_injected_len` as the byte cursor into raw committed ASR text. Route both append and replacement commands through one FIFO worker, while a separate asynchronous Ollama path produces validated candidates. Replacement state tracks the exact rendered suffix plus the target HWND, focused control, and physical-input generation; any mismatch silently drops the candidate.

**Tech Stack:** Rust 2021, Tauri 2, reqwest/Ollama HTTP API, Windows `user32` APIs through the existing `windows` crate, serde/serde_json, existing Enigo clipboard injection.

## Global Constraints

- `refine_enabled` defaults to `false`; every new setting has a serde default.
- Ollama is local-only at `127.0.0.1:11434`; no API key, cloud dependency, or tool calling.
- A configured `refine_model` is exact and has no fallback when unavailable.
- Automatic selection uses live `/api/tags`, rejects `:cloud` and embedding-name patterns, then applies `gemma4 > qwen3.5 > qwen3 > llama3.1 > mistral > phi4`; no match means skip.
- Log the selected model once per dictation run and never log transcript content unless `debug_mode`.
- Sentence calls use temperature `0`, a fixed seed, and an approximately four-second hard timeout.
- Validate numbers, negations, rare terms, invented content, and LCS order before any replacement.
- Before each replacement require unchanged foreground HWND, focus HWND, physical-input generation, and exact replacement character count.
- A failed prerequisite, Ollama call, timeout, or validator gate is silently discarded without UI output.
- Append and replacement keystrokes are serialized by one worker.
- Existing branding, updater, licenses, and `tooling/` remain untouched.

---

### Task 1: Text-fidelity validator

**Files:**
- Create: `apps/local-voice/src-tauri/src/refinement/validator.rs`
- Create: `apps/local-voice/src-tauri/src/refinement/mod.rs`
- Modify: `apps/local-voice/src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `pub(crate) fn validate(original: &str, candidate: &str) -> Result<(), ValidationFailure>`
- Produces: `ValidationFailure::{Numbers, Negations, RareTerms, InventedContent, Order}`

- [ ] **Step 1: Write failing validator tests**

Add table-driven tests that assert:

```rust
assert!(validate("Ich brauche zweiundzwanzig Teile.", "Ich brauche 22 Teile.").is_ok());
assert_eq!(validate("Ich brauche 22 Teile.", "Ich brauche 23 Teile."), Err(ValidationFailure::Numbers));
assert_eq!(validate("Das ist nicht nichts.", "Das ist nichts nichts."), Err(ValidationFailure::Negations));
assert!(validate("Keinen Fehler machen.", "Kein Fehler machen.").is_ok());
assert_eq!(validate("OpenAI nutzt RTX4090.", "OpenAI nutzt Hardware."), Err(ValidationFailure::RareTerms));
assert_eq!(validate("Der Server verarbeitet Daten.", "Der Server analysiert Daten."), Err(ValidationFailure::InventedContent));
assert_eq!(validate("eins zwei drei vier fünf sechs sieben acht neun zehn", "zehn neun acht sieben sechs fünf vier drei zwei eins"), Err(ValidationFailure::Order));
```

- [ ] **Step 2: Run the focused test and confirm RED**

Run:

```powershell
cargo test --lib refinement::validator::tests
```

Expected: compilation fails because the module and validator API do not exist.

- [ ] **Step 3: Implement tokenization and all five gates**

Implement:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidationFailure {
    Numbers,
    Negations,
    RareTerms,
    InventedContent,
    Order,
}

pub(crate) fn validate(original: &str, candidate: &str) -> Result<(), ValidationFailure> {
    validate_numbers(original, candidate)?;
    validate_negations(original, candidate)?;
    validate_rare_terms(original, candidate)?;
    validate_no_invention(original, candidate)?;
    validate_order(original, candidate)?;
    Ok(())
}
```

Use canonical integer strings for standalone digits and German number words, canonical `kein` for every `kein*` flexion, exact preservation for URLs/mail/paths/acronyms/Binnenmajuskel/alphanumeric/long hapax tokens, multiset containment for candidate content tokens, and `LCS / original_token_count >= 0.90`.

- [ ] **Step 4: Run focused tests and confirm GREEN**

Run:

```powershell
cargo test --lib refinement::validator::tests
```

Expected: every validator test passes.

### Task 2: Settings and deterministic live model selection

**Files:**
- Modify: `apps/local-voice/src-tauri/src/settings.rs`
- Create: `apps/local-voice/src-tauri/src/refinement/ollama.rs`
- Modify: `apps/local-voice/src-tauri/src/refinement/mod.rs`

**Interfaces:**
- Consumes: `AppSettings`
- Produces: `select_model(models: &[OllamaModel], configured: Option<&str>) -> Option<String>`
- Produces: `OllamaRefiner::resolve_model` and `OllamaRefiner::refine`

- [ ] **Step 1: Write failing setting and selection tests**

Add literal fixtures proving:

```rust
assert!(!get_default_settings().refine_enabled);
assert_eq!(get_default_settings().refine_model, None);
assert_eq!(select_model(&models, Some("mistral:7b")).as_deref(), Some("mistral:7b"));
assert_eq!(select_model(&models, Some("missing")), None);
assert_eq!(select_model(&models, None).as_deref(), Some("gemma4:12b"));
assert_eq!(select_model(&cloud_and_embeddings, None), None);
```

The fixture contains `nomic-embed-text:latest`, `kimi-k3:cloud`, `qwen3-vl:235b-cloud`, `qwen3:4b`, and `gemma4:12b`.

- [ ] **Step 2: Run focused tests and confirm RED**

Run:

```powershell
cargo test --lib refinement::ollama::tests settings::tests::default_refinement_is_disabled
```

Expected: missing fields and selection function cause compilation failure.

- [ ] **Step 3: Add serde-default settings**

Add:

```rust
#[serde(default = "default_refine_enabled")]
pub refine_enabled: bool,
#[serde(default)]
pub refine_model: Option<String>,
#[serde(default = "default_refine_sentence_timeout_ms")]
pub refine_sentence_timeout_ms: u64,
#[serde(default = "default_refine_final_timeout_ms")]
pub refine_final_timeout_ms: u64,
```

Defaults are `false`, `None`, `4000`, and `12000`.

- [ ] **Step 4: Implement `/api/tags`, filtering, selection, and `/api/generate`**

Deserialize the live tags response, filter lowercase names containing `:cloud`, `embed`, `bge`, `gte`, `nomic-embed`, or `all-minilm`, and scan the preference list in order against the remaining installed names. An explicit setting must exactly equal an installed name and must never fall back.

Send `/api/generate` with:

```rust
GenerateRequest {
    model,
    system: STATIC_SYSTEM_PROMPT,
    prompt: serde_json::to_string(transcript)?,
    stream: false,
    options: GenerateOptions { temperature: 0.0, seed: 424242 },
}
```

Wrap both tags and generation inside the stage timeout and parse only the response text. Do not include tools.

- [ ] **Step 5: Run focused tests and confirm GREEN**

Run:

```powershell
cargo test --lib refinement::ollama::tests settings::tests
```

Expected: selection and defaults pass, including exact configured-model behavior.

### Task 3: Windows replacement safety

**Files:**
- Modify: `apps/local-voice/src-tauri/Cargo.toml`
- Modify: `apps/local-voice/src-tauri/src/input.rs`

**Interfaces:**
- Produces: `ReplacementContext { foreground: isize, focus: isize, physical_generation: u64 }`
- Produces: `capture_replacement_context() -> Option<ReplacementContext>`
- Produces: `send_select_left(enigo: &mut Enigo, count: usize) -> Result<(), String>`

- [ ] **Step 1: Write failing pure guard tests**

Test a pure equality predicate:

```rust
assert!(replacement_context_matches(captured, captured));
assert!(!replacement_context_matches(captured, ReplacementContext { focus: 2, ..captured }));
assert!(!replacement_context_matches(captured, ReplacementContext { physical_generation: 8, ..captured }));
```

- [ ] **Step 2: Run focused tests and confirm RED**

Run:

```powershell
cargo test --lib input::tests
```

Expected: missing context and predicate cause compilation failure.

- [ ] **Step 3: Implement Windows hooks and focus capture**

Enable `Win32_UI_Input_KeyboardAndMouse`. Start one low-level keyboard/mouse hook thread and increment an atomic generation only for events without `LLKHF_INJECTED`/`LLMHF_INJECTED`. Capture `GetForegroundWindow` and `GetGUIThreadInfo(0).hwndFocus`; return `None` if either handle is null or hook setup failed. Non-Windows implementations return `None`, so replacement remains fail-closed.

Implement Shift+Left as one held Shift key plus exactly `count` left-arrow clicks, then release Shift even on error.

- [ ] **Step 4: Run focused tests and confirm GREEN**

Run:

```powershell
cargo test --lib input::tests
```

Expected: pure safety comparisons pass on every platform.

### Task 4: Unified injection and replacement worker

**Files:**
- Create: `apps/local-voice/src-tauri/src/refinement/injection.rs`
- Modify: `apps/local-voice/src-tauri/src/refinement/mod.rs`
- Modify: `apps/local-voice/src-tauri/src/managers/transcription.rs`

**Interfaces:**
- Produces: `InjectionCommand::{Begin, Append, ReplaceSentence, PrepareFinal, ReplaceFinal, Cancel}`
- Produces: `InjectionHandle`
- Consumes: validated candidate strings and `ReplacementContext`

- [ ] **Step 1: Write failing state-machine tests**

Exercise the real state transition logic without Enigo:

```rust
let mut state = InjectionRunState::begin(7);
state.record_append("Erster Satz.", context);
assert_eq!(state.sentence_replacement("Erster Satz.", "Erster guter Satz.", context), Some(("Erster Satz.".into(), "Erster guter Satz.".into())));
state.record_append(" Danach.", context);
assert_eq!(state.sentence_replacement("Erster Satz.", "Erster guter Satz.", context), None);
assert_eq!(state.prepare_final(context).unwrap().text, "Erster Satz. Danach.");
```

Also prove changed HWND, focus, generation, wrong run ID, and sealed runs all reject replacement.

- [ ] **Step 2: Run focused tests and confirm RED**

Run:

```powershell
cargo test --lib refinement::injection::tests
```

Expected: missing state machine causes compilation failure.

- [ ] **Step 3: Implement FIFO append/replacement commands**

Move the current static string queue into a command queue. `Append` keeps the existing clipboard write, Ctrl+V, and 120 ms target-service delay. It records a replacement context only when context before and after the injected paste matches and no physical generation changed.

For `ReplaceSentence`, require the current rendered text to end exactly with the original sentence; select exactly `original.chars().count()` positions and paste the validated candidate. If later raw growth exists, discard instead of rewriting a larger suffix.

`PrepareFinal` seals sentence replacement, waits behind all earlier appends, and returns the current rendered text/context. `ReplaceFinal` requires that exact snapshot, selects the entire rendered character count, and pastes the validated final text. State length changes update rendered state only; `stream_injected_len` remains the raw committed byte offset.

- [ ] **Step 4: Run focused tests and confirm GREEN**

Run:

```powershell
cargo test --lib refinement::injection::tests
```

Expected: all ordering and fail-closed state tests pass.

### Task 5: Sentence-stage integration

**Files:**
- Create: `apps/local-voice/src-tauri/src/refinement/sentences.rs`
- Modify: `apps/local-voice/src-tauri/src/refinement/mod.rs`
- Modify: `apps/local-voice/src-tauri/src/managers/transcription.rs`

**Interfaces:**
- Produces: `complete_sentence_ranges(text: &str, after: usize) -> Vec<Range<usize>>`
- Consumes: raw append byte offsets, run ID, `OllamaRefiner`, validator, unified injection handle

- [ ] **Step 1: Write failing sentence-boundary tests**

Cover German punctuation, decimal numbers, and incomplete tails:

```rust
assert_eq!(sentences("Das geht. Weiter"), vec!["Das geht."]);
assert_eq!(sentences("Der Wert ist 3.14. Weiter"), vec!["Der Wert ist 3.14."]);
assert!(sentences("Noch nicht fertig").is_empty());
```

- [ ] **Step 2: Run focused tests and confirm RED**

Run:

```powershell
cargo test --lib refinement::sentences::tests
```

Expected: missing splitter causes compilation failure.

- [ ] **Step 3: Integrate asynchronous sentence refinement**

At `start_stream`, allocate a monotonically increasing run ID, enqueue `Begin`, reset the scheduled raw byte cursor, and resolve/log the model at most once for the run. After each raw committed append, discover newly complete sentences and spawn local refinement without waiting in the stream worker. Validate each response, log only the gate name unless debug mode, and enqueue `ReplaceSentence`.

Do not change `stream_injected_len` after replacement. Continue storing `committed.len()` after every appended raw delta.

- [ ] **Step 4: Run focused and transcription tests**

Run:

```powershell
cargo test --lib refinement::sentences::tests managers::transcription::tests
```

Expected: sentence detection and existing streaming projections pass.

### Task 6: Final full-pass and cancellation

**Files:**
- Modify: `apps/local-voice/src-tauri/src/managers/transcription.rs`
- Modify: `apps/local-voice/src-tauri/src/actions.rs`
- Modify: `apps/local-voice/src-tauri/src/refinement/mod.rs`

**Interfaces:**
- Produces: `TranscriptionManager::refine_final_injected_text() -> impl Future<Output = ()>`
- Consumes: `PrepareFinal` snapshot, run model, final timeout, validator, `ReplaceFinal`

- [ ] **Step 1: Write failing lifecycle tests**

Add pure lifecycle assertions that `PrepareFinal` seals sentence work, `Cancel` rejects late candidates, and `ReplaceFinal` only accepts the exact prepared snapshot/run/context.

- [ ] **Step 2: Run focused tests and confirm RED**

Run:

```powershell
cargo test --lib refinement::injection::tests
```

Expected: new lifecycle assertions fail until final/cancel transitions exist.

- [ ] **Step 3: Integrate final pass**

After successful `finalize_stream` and before hiding the overlay, call the final refinement only when both refinement and continuous injection are active. Await the prepared FIFO snapshot, refine that actual rendered text with the run's already resolved model, validate, then enqueue/await `ReplaceFinal`. Wrap the future in the existing cancellation poll helper.

`cancel_stream` enqueues `Cancel` for the run so all late sentence/final results are ignored.

- [ ] **Step 4: Run library tests**

Run:

```powershell
cargo test --lib
```

Expected: baseline tests plus new tests all pass.

### Task 7: Format and final verification

**Files:**
- Verify all changed Rust and manifest files.

- [ ] **Step 1: Format**

Run:

```powershell
cargo fmt --check
```

If it reports diffs, run `cargo fmt` and repeat the check.

- [ ] **Step 2: Release build**

Run:

```powershell
cargo build --release
```

Expected: exit code 0. If the documented Visual Studio generator cache mismatch occurs, remove only resolved `target/*/build/transcribe-cpp-sys-*` directories and `%LOCALAPPDATA%\tcs`, then rerun.

- [ ] **Step 3: Complete library test suite**

Run:

```powershell
cargo test --lib
```

Expected: exit code 0, no failed tests, and passing count at least 160.

- [ ] **Step 4: Inspect scope**

Run:

```powershell
git status --short
git diff --check
git diff --stat
```

Expected: only refinement feature files, the plan, `Cargo.toml`, and relevant Rust integration files changed; no branding, updater, license, or `tooling/` changes.
