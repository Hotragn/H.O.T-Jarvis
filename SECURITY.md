# Security Policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems. Instead use GitHub's private vulnerability reporting on this repository ("Security" tab → "Report a vulnerability"). You should get a first response within 7 days.

## Scope notes

- H.O.T-Jarvis is local-first: memory, notes, and logs live on the user's machine. Anything that exfiltrates that data, escapes the app's allowed directories, or executes outside its sandbox is a vulnerability.
- API keys are read only from environment variables / a gitignored `.env`. Any code path that could log, store, or transmit a key is a vulnerability.
- Actions with external side effects must pass through the approval gates; a bypass is a vulnerability even if the action itself seems harmless.

## Supported versions

Pre-1.0: only the latest release / `main` receives fixes.
