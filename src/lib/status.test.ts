import { describe, expect, it } from "vitest";
import type { Status } from "./ipc";
import { describeStatus } from "./status";

function status(overrides: Partial<Status>): Status {
  return {
    providers: [],
    ready: false,
    onboarding: null,
    message_count: 0,
    fact_count: 0,
    ...overrides,
  };
}

describe("describeStatus", () => {
  it("shows connecting while status is unknown", () => {
    expect(describeStatus(null)).toEqual({ label: "connecting…", tone: "warn" });
  });

  it("prefers a reachable local model", () => {
    const s = status({
      ready: true,
      providers: [
        { id: "ollama", configured: true, reachable: true, model: "llama3.2" },
        { id: "groq", configured: true, reachable: null, model: "llama-3.3" },
      ],
    });
    expect(describeStatus(s)).toEqual({ label: "local · llama3.2", tone: "ok" });
  });

  it("falls back to a configured cloud provider when local is down", () => {
    const s = status({
      ready: true,
      providers: [
        { id: "ollama", configured: true, reachable: false, model: "llama3.2" },
        { id: "groq", configured: true, reachable: null, model: "llama-3.3" },
      ],
    });
    expect(describeStatus(s)).toEqual({ label: "groq · llama-3.3", tone: "ok" });
  });

  it("warns when nothing can answer", () => {
    const s = status({
      providers: [
        { id: "ollama", configured: true, reachable: false, model: "llama3.2" },
        { id: "groq", configured: false, reachable: null, model: "llama-3.3" },
      ],
    });
    expect(describeStatus(s).tone).toBe("warn");
  });
});
