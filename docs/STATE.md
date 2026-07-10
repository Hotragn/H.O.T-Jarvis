# Project State

*Updated: 2026-07-05 (bootstrap session)*

## Where the project is

First-session bootstrap in progress on branch `feat/bootstrap-core`. Repo went from empty (LICENSE + README stub) to: governance docs, Tauri v2 + React scaffold, Rust core (router / memory / notes tool) with unit tests, bare HUD (themes + waveform + chat), CI workflows.

## Auto-merge

**Disabled.** The owner merges all PRs. (Flip this line to "Enabled for low-risk green-CI changes" to change that.)

## Environment facts (this machine)

- Windows 11 **ARM64** (Snapdragon), Node 24, Rust 1.93, Python 3.14. No `gh` CLI. No Ollama installed yet. No free-tier API keys in env yet — the no-provider onboarding path is the real first-run experience here.
- **Toolchain quirk:** VS Build Tools 2022/2026 are installed but the *ARM64-target* C++ tools are missing (no `Hostarm64\arm64\link.exe`), so the default `aarch64-pc-windows-msvc` toolchain can't link. Workaround in use: `rustup run stable-x86_64-pc-windows-msvc cargo <cmd>` (x64 binaries run via emulation). Proper fix (owner, admin shell): in the VS Installer add "C++ ARM64/ARM64EC build tools" (component `Microsoft.VisualStudio.Component.VC.Tools.ARM64`).
- Also: don't run cargo from Git Bash here — GNU coreutils `link` shadows MSVC `link.exe`. Use PowerShell.
- Git remote: https://github.com/Hotragn/H.O.T-Jarvis (push access assumed via credential manager).
- Prompt/agent files (`CLAUDE.md`, `docs/agentbrief*`, `.claude/`) are **gitignored by owner request** — never commit them.

## Milestones verified

- 2026-07-06: Owner ran `npm run tauri dev` with live Ollama (llama3.2) — app works end-to-end on the target machine. PR #1 merged; branch auto-cleanup workflow confirmed working.

## Next 3 tasks

1. Replay v1 (§5.4, last hero feature): deterministic re-run of a session from the event log; undo for reversible actions.
2. Record the README GIF (chat + skills + authoring + events + reflection + confidence gauge) — needs the owner or a screen-recording tool.
3. Update README feature list: hero features 1-3 are now real, not planned.

## UI state (2026-07-06)

Jarvis-style HUD shipped on `feat/bootstrap-core`: ArcCore visualizer, tab navigation (chat/notes/memory), Ctrl+K command palette, live telemetry (sysinfo CPU/RAM/uptime + memory counts + clock), notes and memory-browser views wired to the existing Rust commands. Browser preview shows standby placeholders by design; full data needs the Tauri shell.

## Blockers / waiting on owner

- PR review + merge of `feat/bootstrap-core` (auto-merge is off).
- Branch protection on `main` with required status checks should be enabled in GitHub settings (can't be done from this machine without `gh`).
- Optional: install Ollama (`ollama pull llama3.2`) or drop a free Groq/OpenRouter key into `.env` to light up real inference.
