# Changelog

All notable changes to H.O.T-Jarvis. Format loosely follows [Keep a Changelog](https://keepachangelog.com/); versions follow semver once there's a runnable core release.

## [Unreleased]

### Added
- Bootstrap: Tauri v2 + React desktop shell with themed HUD (dark/light design tokens), animated activity waveform, chat view.
- Model router with Ollama (local-first) → Groq → OpenRouter `:free` fallback and a friendly no-provider onboarding path.
- Persistent SQLite memory (conversation history + key-value profile) with schema migrations; survives restarts; export and wipe operations.
- Built-in local notes tool scoped to the app data directory.
- CI: lint, type-check, tests, secret scan, build; merged-branch cleanup workflow.
- Repo governance: README, ROADMAP, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, decision log.
