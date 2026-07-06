# Contributing

Thanks for helping build H.O.T-Jarvis. Ground rules:

## The one hard product rule

The assistant must stay **free to run**. PRs that require a paid API, add a paid dependency, or nudge users toward paying will not be merged. Free tiers are fine; local-first is preferred.

## Workflow

1. Branch from `main` (`feat/...`, `fix:...`, `docs/...`). Never commit to `main` directly.
2. Keep PRs small — one concern, with tests for any new behavior.
3. CI must be green (lint, type-check, tests, secret scan, build) before merge.
4. Use conventional commit messages: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `ci:`.
5. Merged feature branches get deleted automatically.

## Code standards

- **Rust** (`src-tauri/`): `cargo fmt`, `cargo clippy -- -D warnings`, unit tests per module. Core logic lives in `src-tauri/src/core/` and stays independent of Tauri so it's testable.
- **TypeScript** (`src/`): `npm run lint`, `npm run typecheck`, `npm test`. Drive all styling through the design tokens in `src/styles/tokens.css`.
- No secrets, keys, or personal data in the repo — ever. `.env` is gitignored; extend `.env.example` with placeholders instead.
- Don't copy substantial code from other projects. Original implementations only; cite ideas and papers in `docs/DECISIONS.md`.

## Local dev

```bash
npm install
npm run tauri dev        # full app
npm test                 # frontend tests
cd src-tauri && cargo test   # core tests
```

## Questions

Open a GitHub issue or discussion.
