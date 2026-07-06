import { describe, expect, it } from "vitest";
import type { AppEvent } from "./ipc";
import { eventDomain, summarizeEvent } from "./events";

function event(kind: string, payload: Record<string, unknown>): AppEvent {
  return { id: 1, ts: 1751800000, kind, payload };
}

describe("summarizeEvent", () => {
  it("summarizes each known kind", () => {
    expect(summarizeEvent(event("app.started", { version: "0.1.0" }))).toBe(
      "session started · v0.1.0",
    );
    expect(summarizeEvent(event("chat.user", { text: "hello" }))).toBe("hello");
    expect(
      summarizeEvent(
        event("chat.assistant", {
          text: "hi",
          provider: "ollama",
          model: "llama3.2",
          duration_ms: 1500,
        }),
      ),
    ).toBe("hi  [ollama · llama3.2 · 1500ms]");
    expect(summarizeEvent(event("note.saved", { slug: "groceries" }))).toBe(
      'saved note "groceries"',
    );
    expect(summarizeEvent(event("memory.wiped", {}))).toContain("erased");
  });

  it("truncates long chat text", () => {
    const long = "x".repeat(300);
    const summary = summarizeEvent(event("chat.user", { text: long }));
    expect(summary.length).toBeLessThanOrEqual(96);
    expect(summary.endsWith("…")).toBe(true);
  });

  it("falls back to raw payload for unknown kinds", () => {
    expect(summarizeEvent(event("future.thing", { a: 1 }))).toBe('{"a":1}');
  });
});

describe("eventDomain", () => {
  it("extracts the domain prefix", () => {
    expect(eventDomain("chat.user")).toBe("chat");
    expect(eventDomain("app.started")).toBe("app");
    expect(eventDomain("plain")).toBe("plain");
  });
});
