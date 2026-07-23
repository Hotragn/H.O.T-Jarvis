---
title: Contributing
description: How to propose changes — workflow, code standards, and running the tests.
---

Contributions are welcome. One hard rule frames everything: the assistant must
stay **free to run**. Changes that require a paid API or nudge users toward
paying won't be merged.

## Workflow

1. Branch from `main` (`feat/...`, `fix:...`, `docs/...`). Never commit to `main`.
2. Keep pull requests small — one concern, with tests for new behavior.
3. CI must be green (lint, type-check, tests, secret scan, build) before merge.
4. Use conventional commits: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `ci:`.
5. Merged feature branches are deleted automatically.

## Code standards

- **Rust** (`src-tauri/`): `cargo fmt`, `cargo clippy -- -D warnings`, a unit
  test per module. Core logic lives in `src-tauri/src/core/` and stays
  independent of Tauri so it's testable.
- **TypeScript** (`src/`): `npm run lint`, `npm run typecheck`, `npm test`. Drive
  styling through the design tokens; keep animation transform/opacity-only and
  respect `prefers-reduced-motion`.
- No secrets or personal data in the repo, ever. Extend `.env.example` with
  placeholders instead.
- Original implementations only — cite ideas and papers, don't copy code.

## Running everything

```bash frame="terminal"
npm install
npm run tauri dev            # the full app
npm test                     # frontend tests
cd src-tauri && cargo test   # core tests
```

## Documentation

These docs live in `docs-site/`. Each page has an **Edit this page** link that
opens the source on GitHub. Use active voice, be concrete, and prefer clarity
over marketing.

Full detail is in the repository's `CONTRIBUTING.md`, `SECURITY.md`, and
`CODE_OF_CONDUCT.md`.
