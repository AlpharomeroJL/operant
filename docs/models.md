# Models

A plain guide to running Operant's planner, grounder, speech-to-text, and
text-to-speech entirely on your own machine, sized for a 16 GB VRAM consumer
GPU (the reference card used elsewhere in this repo is an RTX 4060 Ti 16 GB).
It also covers swapping in your own UI-TARS-class grounder, and how each row
of the provider quirk table (`crates/orchestrator/src/backends/quirks.rs`,
spec'd in `docs/specs/backends.md`) maps to a working configuration.

Nothing here requires a network call unless you make one on purpose.
Building and testing this crate normally (`cargo build`, `cargo test`)
never contacts a model endpoint; see "Verifying your setup" below for the
one opt-in exception.

## Quick answer

| Role | Default local model | Served by | Disk | VRAM |
|---|---|---|---|---|
| Planner | Qwen2.5-7B-Instruct, Q4_K_M GGUF | Ollama | about 4.7 GB | about 5.5 GB |
| Grounder (vision) | UI-TARS-1.5-7B, Q4_K_M GGUF | llama.cpp server | about 5.5 GB | about 6.5 GB |
| STT | whisper.cpp, small.en, quantized | in-process / CPU | about 0.5 GB | about 0.5 GB, or none on CPU |
| TTS | Kokoro-82M | in-process / CPU | about 0.3 GB | about 0.3 GB, or none on CPU |

These are practical planning numbers, not a certified benchmark. Quantization
choice, context length, and OS overhead move actual usage by a GB or so in
either direction. Treat the table as a budget, not a guarantee.

## The default local stack

Every role in `contracts/model_backend.md` (planner, grounder, STT, TTS) is
independently swappable. The pairing below is simply the one that fits
comfortably in 16 GB with headroom to spare, and is what to reach for first
if you have no other preference.

### Planner: Qwen2.5-7B-Instruct

A 7B instruction-tuned model with reliable tool-calling, which the planner
role needs for the trivial-tool-schema step of the capability probe
(`backends::client::HttpBackend::probe`) and for real tool use once a
workflow is exploring. Pull it through Ollama:

```powershell
ollama pull qwen2.5:7b-instruct
```

Any similarly sized (7 to 8B), instruction-tuned, tool-calling model works
just as well; Qwen2.5-7B-Instruct is the one this guide sizes for because its
quantized footprint leaves the most room for the grounder alongside it.

### Grounder: a UI-TARS-class vision model

`docs/ARCHITECTURE.md`'s C3 describes the grounder as "a local VLM endpoint
(Ollama/llama.cpp, UI-TARS-class grounder) or API vision backend." UI-TARS-1.5-7B
(or whichever UI-TARS-class release is current when you read this) is the
recommended default: it is trained specifically for GUI grounding, so it
tends to answer "where is the Save button" far more reliably than a
general-purpose vision-language model of the same size.

Serve it through **llama.cpp server** rather than Ollama. Ollama's model
library does not consistently carry a UI-TARS GGUF with a working vision
Modelfile, while llama.cpp server accepts any compatible GGUF directly and
is already a first-class row in the quirk table (`llamacpp`):

```powershell
llama-server -m ui-tars-1.5-7b-q4_k_m.gguf --mmproj ui-tars-1.5-7b-mmproj.gguf --port 8080
```

Point Operant at it with provider id `llamacpp` (see "Configuring a
provider" below); the capability probe will detect vision support on its own
by sending the standard 1x1 PNG probe request, so there is nothing to flag
by hand.

### STT: whisper.cpp, small.en, quantized

The default per `docs/specs/voice.md`: 16 kHz mono input, push-to-talk with a
300 ms tail, VAD trim before inference. Small enough to run on CPU with
acceptable latency; only budget GPU memory for it if you have room to spare
after the planner and grounder.

### TTS: Kokoro-class, one default voice

Also per `docs/specs/voice.md`. Kokoro-82M is small enough that, like STT, it
usually is not worth dedicating VRAM to: the voice sidecar loads it lazily on
first use and yields it back within 2 seconds when the grounder needs
headroom (the C1 VRAM arbitration broker; `sidecars/voice/src/vram.js`), so
co-residency with the grounder is a convenience, not a requirement.

### Why the budget fits

| Loaded at once | VRAM |
|---|---|
| Planner | about 5.5 GB |
| Grounder | about 6.5 GB |
| STT (if on GPU) | about 0.5 GB |
| TTS (if on GPU) | about 0.3 GB |
| **Total, worst case** | **about 12.8 GB** |

That leaves roughly 3 GB of headroom on a 16 GB card before counting the
several hundred MB to 1 GB the OS and desktop compositor typically hold, and
before the voice sidecar's yield-on-demand behavior even kicks in. In
practice the realistic simultaneous floor is planner plus grounder alone
(about 12 GB), since STT and TTS are lazy-loaded and small enough to prefer
CPU.

If you are tighter on VRAM than 16 GB, drop to a 4-bit quant of a smaller
grounder (a 3B to 4B UI-TARS-class model, where available) before dropping
the planner; grounding accuracy degrades faster than planning quality does at
small sizes, so protect the grounder's parameter count first if you have to
choose.

## Bring your own UI-TARS grounder

The grounder role is a plain `ModelBackend`, so any UI-TARS-class (or other
vision-capable) model you already run is a drop-in replacement, not a fork.
There is no separate "grounder config"; it is the same `BackendConfig` and
quirk-table row every other role uses, with the `grounder` role tag on the
request instead of `planner` (`backends::types::RequestRole`).

Three ways to point Operant at your own grounder:

1. **Already serving an OpenAI-compatible endpoint** (llama.cpp server,
   vLLM, LM Studio, or your own reverse proxy in front of something else):
   use the matching provider id (`llamacpp`, `vllm`, `lmstudio`) if the
   default `base_url` matches where you run it, or `generic` with
   `BackendConfig::with_base_url` if it does not.
2. **A different port or host** (remote GPU box, container, WSL): every
   provider row's default `base_url` is overridable per instance; nothing in
   the quirk table is a hardcoded assumption about `localhost`.
3. **A hosted vision API instead of a local model**: any provider in the
   table with `vision` support works the same way; swap the provider id and
   supply an API key.

Whichever route, the capability probe (`docs/specs/backends.md`'s "Capability
probe on configure") is what decides whether a backend is fit for the
grounder role, not a name you have to get right: it sends a real request
with a 1x1 PNG content part and records whether the response comes back
clean. Assigning a non-vision backend to the grounder role fails with a
plain-language explanation
(`backends::types::BackendProfile::explain_role_mismatch`):
"This model cannot see images, so it cannot find things on screen. Use it
for planning, or pick a vision model."

## Configuring a provider from the quirk table

Every provider is one row of `backends::quirks::ProviderQuirks`: a base URL,
an auth shape, a request dialect, a streaming format, a vision encoding, and
the field name the provider uses for max tokens. `client::HttpBackend` and
the `dialect` modules are entirely generic over this data, so pointing at a
new provider, or a self-hosted instance of an existing one, is filling in a
`BackendConfig`, never new code.

| Provider id | Default base URL | Auth | Dialect |
|---|---|---|---|
| `ollama` | `http://localhost:11434/v1` | none | openai |
| `llamacpp` | `http://localhost:8080/v1` | none | openai |
| `lmstudio` | `http://localhost:1234/v1` | none | openai |
| `vllm` | `http://localhost:8000/v1` | none | openai |
| `generic` | none (you must set one) | bearer | openai |
| `openai` | `https://api.openai.com/v1` | bearer | openai |
| `anthropic` | `https://api.anthropic.com/v1` | x-api-key | anthropic |
| `gemini` | `https://generativelanguage.googleapis.com/v1beta` | query param | gemini |
| `deepseek`, `minimax`, `kimi`, `qwen`, `groq`, `mistral`, `xai`, `openrouter` | each provider's own hosted URL | bearer | openai |

The full table, including the streaming format and vision encoding columns,
lives in `crates/orchestrator/src/backends/quirks.rs` and is exercised by
that file's own tests; treat this section as a map, not the source of truth.

### By hand

```rust
use operant_orchestrator::backends::BackendConfig;

// Anthropic: x-api-key auth, native dialect, default base_url from the
// quirk table, so only the model and key need to be supplied.
let planner = BackendConfig::new("anthropic", "claude-sonnet-4-20250514")
    .with_api_key("sk-ant-...");

// A local llama.cpp server on a non-default port: same provider id,
// base_url overridden.
let grounder = BackendConfig::new("llamacpp", "ui-tars-1.5-7b")
    .with_base_url("http://localhost:8090/v1");
```

### From the environment, for a quick reachability check

`crates/orchestrator/src/backends/live_config.rs` resolves a
`BackendConfig` from five environment variables, mainly so this crate's
flagged real-endpoint tests (below) have something to opt into, but usable
by any caller that wants a live backend without wiring a config file:

| Variable | Meaning | Required |
|---|---|---|
| `OPERANT_LIVE_BACKEND` | Opt-in gate. Any value, including empty, enables the rest. | Always, to enable anything |
| `OPERANT_LIVE_PROVIDER` | Provider id from the table above. | No; defaults to `ollama` |
| `OPERANT_LIVE_MODEL` | Model name. | Defaulted for local providers; required for hosted ones |
| `OPERANT_LIVE_BASE_URL` | Overrides the provider's default `base_url`. | Required only when the provider has no default (`generic`) |
| `OPERANT_LIVE_API_KEY` | API key, where the provider needs one. | Required for bearer/x-api-key/query-param providers |

```rust
use operant_orchestrator::backends::{LiveBackendConfig, LiveConfigError};

// Reads OPERANT_LIVE_BACKEND and friends from the process environment.
// With none of them set this always reports NotEnabled, which is the
// same skip path every flagged test in this crate relies on.
match LiveBackendConfig::from_env() {
    Ok(live) => println!("configured: {}", live.backend_config.provider_id),
    Err(LiveConfigError::NotEnabled) => println!("no live backend configured"),
    Err(e) => eprintln!("misconfigured: {e}"),
}
```

## Verifying your setup

`crates/orchestrator/src/backends/live_endpoint_tests.rs` has two tests that
actually complete a prompt and run a probe against a real endpoint. They
compile and pass with zero network I/O in a normal build; to point them at a
real local model:

```powershell
ollama pull llama3.2
$env:OPERANT_LIVE_BACKEND = "1"
cargo test -p operant-orchestrator --features real-transport live_endpoint -- --nocapture
```

Against a hosted provider instead:

```powershell
$env:OPERANT_LIVE_BACKEND = "1"
$env:OPERANT_LIVE_PROVIDER = "openai"
$env:OPERANT_LIVE_MODEL = "gpt-4o-mini"
$env:OPERANT_LIVE_API_KEY = "sk-..."
cargo test -p operant-orchestrator --features real-transport live_endpoint -- --nocapture
```

Leave `OPERANT_LIVE_BACKEND` unset and the same command still compiles and
passes; it just prints why it skipped instead of talking to anything.

This is a spot check for your own machine, not a substitute for the
model-reachability check `operant doctor` runs (spec'd in
`docs/specs/zero-code.md`, implemented in `crates/doctor`), which is the
supported way to diagnose a broken model setup end to end.

## See also

- `docs/specs/backends.md`: the one-paragraph spec this module implements.
- `contracts/model_backend.md`: the binding trait signatures and quirk
  table field list.
- `docs/specs/voice.md`: the STT/TTS defaults and VRAM yield protocol in
  full.
- `docs/ARCHITECTURE.md`, section C6: where the backend layer sits in the
  rest of Operant.
