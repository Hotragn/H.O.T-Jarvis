---
title: Skill manifest
description: The on-disk format of a skill — manifest fields, the code + test contract, versioning, and the sandbox.
---

A skill is a directory under `<data_dir>/skills/<slug>/`:

```
<slug>/
├─ manifest.json      # metadata + test status
├─ skill.rhai         # fn run(input)
├─ test.rhai          # fn test()
└─ history/v<N>/      # archived previous versions
```

## Manifest

```ts
{
  name: string;        // slug (lowercase, alphanumerics + single hyphens)
  version: number;     // integer, bumped on every update
  description: string;
  created_at: number;
  updated_at: number;
  test_status: { status: "passed" } | { status: "failed"; detail: string };
}
```

## The contract

- **`skill.rhai`** defines `fn run(input)` — takes one string, returns a value.
- **`test.rhai`** defines `fn test()` — returns `true` when the skill is correct,
  and calls `run(...)` with at least one concrete example.

On every save and re-test, `run` + `test` are compiled together and `test()` is
executed. `Passed` → the skill is runnable. `Failed` → it's **flagged** and
`run_skill` refuses it until it passes.

## The language

Skills are written in [Rhai](https://rhai.rs) — a small, embedded scripting
language. Useful string methods: `.len()`, `.to_upper()`, `.to_lower()`,
`.trim()`, `.contains(s)`, `.replace(a, b)`, `.split(s)`, `.sub_string(start, len)`.

## The sandbox

Rhai is sandboxed by construction — scripts see only language built-ins (no
filesystem, no network). Each execution runs under hard caps: 200,000 operations,
bounded call depth, and bounded string/array/map sizes. A runaway loop
terminates instead of hanging the assistant.

## Versioning & rollback

Updating a skill archives the outgoing `skill.rhai` / `test.rhai` to
`history/v<N>/` and increments the version. A rollback restores the previous
version's sources as a **new** version (a revert, preserving the chain), or
deletes the skill if it's on v1 with nothing to revert to.
