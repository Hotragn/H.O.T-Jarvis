import { describe, expect, it } from "vitest";
import {
  followUpSeconds,
  isCapturing,
  nextHint,
  phaseBadge,
  phaseLabel,
  wakePhraseError,
} from "./handsfree";
import type { VoicePhase, VoiceSession } from "./ipc";

const ALL_PHASES: VoicePhase[] = [
  "off",
  "waiting",
  "listening",
  "thinking",
  "speaking",
  "follow_up",
];

const session = (over: Partial<VoiceSession> = {}): VoiceSession => ({
  phase: "waiting",
  wake_phrase: "hey jarvis",
  wants_audio: true,
  needs_wake: true,
  follow_up_remaining_ms: 0,
  ...over,
});

describe("phase copy", () => {
  it("has plain-language text for every phase", () => {
    for (const phase of ALL_PHASES) {
      expect(phaseLabel(phase).length).toBeGreaterThan(3);
      expect(phaseBadge(phase).length).toBeGreaterThan(2);
    }
  });

  it("does not claim to be listening when it isn't", () => {
    // A hands-free indicator is a privacy signal; "listening" must only appear
    // when the mic is genuinely open.
    expect(phaseBadge("speaking")).not.toContain("listening");
    expect(phaseBadge("thinking")).not.toContain("listening");
    expect(phaseBadge("off")).not.toContain("listening");
    expect(phaseBadge("listening")).toBe("listening");
    expect(phaseBadge("follow_up")).toBe("listening");
  });
});

describe("isCapturing", () => {
  it("mirrors the core rather than re-deriving the rule", () => {
    expect(isCapturing(session({ wants_audio: true }))).toBe(true);
    expect(isCapturing(session({ phase: "speaking", wants_audio: false }))).toBe(false);
    expect(isCapturing(null)).toBe(false);
  });
});

describe("followUpSeconds", () => {
  it("rounds up inside the window", () => {
    expect(followUpSeconds(session({ phase: "follow_up", follow_up_remaining_ms: 4200 }))).toBe(5);
    expect(followUpSeconds(session({ phase: "follow_up", follow_up_remaining_ms: 1 }))).toBe(1);
  });

  it("returns null outside the window instead of a misleading zero", () => {
    expect(followUpSeconds(session({ phase: "follow_up", follow_up_remaining_ms: 0 }))).toBeNull();
    expect(followUpSeconds(session({ phase: "waiting", follow_up_remaining_ms: 5000 }))).toBeNull();
    expect(followUpSeconds(null)).toBeNull();
  });
});

describe("nextHint", () => {
  it("names the actual wake phrase when one is needed", () => {
    const hint = nextHint(session({ needs_wake: true, wake_phrase: "ok computer" }));
    expect(hint).toContain("ok computer");
  });

  it("tells the user no wake phrase is needed in the follow-up window", () => {
    const hint = nextHint(
      session({ phase: "follow_up", needs_wake: false, follow_up_remaining_ms: 6000 }),
    );
    expect(hint).toContain("follow-up");
    expect(hint).toContain("6s");
    expect(hint).toContain("no wake phrase");
  });

  it("is silent when there is nothing useful to say", () => {
    expect(nextHint(session({ phase: "off" }))).toBeNull();
    expect(nextHint(session({ phase: "thinking", needs_wake: false }))).toBeNull();
    expect(nextHint(null)).toBeNull();
  });
});

describe("wakePhraseError", () => {
  it("mirrors the backend two-word rule so the user hears it first", () => {
    expect(wakePhraseError("hey jarvis")).toBeNull();
    expect(wakePhraseError("  ok   computer  ")).toBeNull();
    expect(wakePhraseError("jarvis")).toContain("two words");
    expect(wakePhraseError("")).toContain("Enter");
    expect(wakePhraseError("   ")).toContain("Enter");
  });
});
