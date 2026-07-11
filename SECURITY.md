# Security Policy

Operant is an open-source, local-first desktop agent that perceives the screen through accessibility trees and vision, and acts through synthesized input. This document describes the security model, scope, and responsible disclosure process.

## In scope

Operant runs entirely on the user's machine and has access to:
- Screen content and accessibility trees of all running applications
- Mouse, keyboard, and clipboard synthesis capabilities
- File system read/write operations (with capability grants)
- Local model execution and API backends configured by the user
- Workflow definitions and execution logs (stored locally)

Security issues in scope:
- **Escape of capability grants**: a compiled workflow exceeds its declared permissions
- **Invariant bypass**: hard safety checks (credential fields, payment/deletion confirmations) can be disabled or circumvented
- **Kill switch failure**: the global panic hotkey does not freeze input synthesis within the 100 ms latency budget
- **Anchor redaction bypass**: sensitive screenshots (password fields, credential dialogs) are stored unredacted on disk
- **Undo journal tampering**: inverse actions for destructive operations are not recorded or are lossy
- **Audit chain compromise**: the hash-chained activity log is not append-only or can be modified retroactively
- **Model injection**: untrusted model backends can read or exfiltrate screen data against the user's intent
- **Compiled workflow tampering**: a signed workflow is accepted after modification or an invalid signature is not rejected
- **Sidecar escape**: a helper process (vision, voice, model broker) breaks out of its sandbox and escalates privileges
- **VRAM broker abuse**: sidecars can allocate unbounded memory or starve other processes
- **Crash data leaks**: crash logs or core dumps contain unredacted screen content

## Out of scope

Not security issues:
- **Model quality**: accuracy of vision grounding or planning (product quality, not security)
- **Denial of service from user intent**: a user running a workflow that consumes all disk space (workflow audit, not a security bug)
- **Network exfiltration via the user's own API key**: if a user configures an untrusted model backend with their real key, that is user misconfiguration, not a product flaw (the doctor will surface key storage concerns)
- **Local privilege escalation**: Operant assumes the user runs as themselves; exploits that jump from the Operant process to system or other-user privilege are OS issues, not ours
- **Physical access attacks**: someone with local admin access or debugger attachment can read anything; this is expected
- **Supply chain**: the build pipeline, website, or distribution channel (report to Anthropic's security team instead)

## Guardian set (runtime-enforced hard invariants)

These four features are implemented below the planner so no model state or workflow logic can disable or circumvent them:

### 1. Kill switch (panic hotkey)

A global hotkey and tray button instantly freeze all input synthesis at the action-execution layer and halt every run.
- **Latency**: under 100 ms, tested in CI
- **Recovery**: explicit human resume or retry, never automatic
- **Tray state**: turns red when engaged
- **Implementation**: interrupt handler at the kernel-interface layer, no model or workflow state involved

### 2. Undo journal

Every write-class action records an inverse where one exists:
- File operations: creation/move/delete recorded as recycle-bin semantics
- Clipboard changes: prior content stored for restoration
- Irreversible actions (sent email, submitted forms): labeled "Cannot undo" in the step view before execution
- "Undo last run" replays inverses in reverse order and narrates in plain English what is restored
- **Storage**: `undo_journal` table keyed by run ID, content-addressed
- **Tested property**: filesystem-diff tests verify recycle-bin restore accuracy

### 3. Anchor redaction

Stored screenshots and vision anchors are redacted before they touch disk:
- Regions flagged as sensitive by the accessibility tree (password fields, credential dialogs) are blacked out
- Redaction pass runs between capture and blob store
- **Tested**: a fixture credential form asserts redaction coverage
- **Scope**: anchors only; full-screen debug logs are marked clearly and kept separate

### 4. Hard safety invariants

The following are enforced in the runtime and cannot be disabled by any workflow manifest:
- **Never type into a credential field without explicit human approval**: every action into a field flagged `isPassword` or similar halts and requires a per-step confirmation
- **Never confirm a payment or deletion dialog without explicit human approval**: dialogs matching payment/confirmation patterns halt and require explicit user action
- **Dry-run default for new and installed workflows**: unsigned or unverified workflows execute in preview-only mode; no side effects are allowed unless the user grants execution permission
- **Zero model inference in replay**: compiled workflows must execute deterministically; if any step would require a model call (e.g., because drift is detected), replay halts and escalates to the user for approval (the drift-repair flow is human-in-the-loop)

## Responsible disclosure

If you discover a security vulnerability in Operant:

1. **Do not open a public issue or discuss it in forums.**
2. **Email a private report to**: [security contact to be added at launch; for now, reach out in Discussions]
3. **Include**:
   - Description of the vulnerability
   - Steps to reproduce (if possible)
   - Potential impact
   - Any suggested fix (optional)
4. **Expected response time**: 
   - Acknowledgment within 5 business days
   - Initial triage and plan within 10 business days
   - Patched release or public advisory within 30 days (sooner for critical issues)
5. **Attribution**: We will credit you by name or anonymously, your choice

After a fix is released publicly, you are welcome to disclose the vulnerability (e.g., blog post, conference talk). We ask that you do not disclose before we have a fix available and have notified users.

## Scope clarifications

### Local-first design

Operant stores all data locally by default. No screen content, trajectories, or workflows leave the machine without explicit user action (e.g., exporting, publishing to the registry, or configuring a cloud model backend). The threat model assumes a single user per machine; multi-user security is not addressed.

### Model backends

If you configure a cloud model (Anthropic Claude, OpenAI, etc.):
- You supply your own API key
- Screen digests (element descriptions, not raw pixels) are sent to that service under their terms of service
- Vision anchors are sent to the vision backend (local or cloud) only when a step requires grounding
- The user controls what model to use; we do not mandate any specific provider

When configured with a local model (Ollama, llama.cpp, etc.), all inference is on-device.

### Accessibility tree and screen reading

Operant reads the accessibility tree to understand screen structure. This is the same data a screen reader accesses. If an application stores secrets in non-accessible hidden fields or obfuscates sensitive regions, Operant may read them. This is not a product bug; it is the nature of screen automation.

## Security guidelines for contributors

- Capability grants are checked at execution time. New actions must validate permissions before acting.
- Hard invariants must never have an escape hatch or workflow override. If you add a new hard invariant, add a regression test and note it in the ADR.
- Sidecars (vision, voice, model broker) run in separate processes to isolate faults. Keep them that way; do not pull sidecar code into the main process.
- The audit log is hash-chained and append-only. Do not add mutation or deletion operations on historical entries.
- Crash logs and debug output must be scanned for unredacted screen content. Use the redaction utilities before writing to disk.
- Test with the kill switch engaged to confirm input synthesis truly freezes (latency test is non-negotiable).

## Acknowledgments

Operant's security model draws from principles of:
- Least privilege (capability grants per workflow)
- Defense in depth (multiple barriers: kills witch, invariants, audit)
- Transparency (all actions recorded, auditable, signable)
- Local control (no default cloud dependencies)

## Questions or feedback?

Open a Discussion or email the maintainer with any questions about this policy.
