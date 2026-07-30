import { describe, expect, it } from "vitest";
import { ago, beatText, haltText } from "./autonomy";

// The panel's whole job is telling the truth about a loop the user can't see.
// These tests pin the wording, because a vague status line is the same failure as
// no status line.

describe("haltText", () => {
  it("names every halt reason in plain words", () => {
    expect(haltText({ reason: "stop_file" })).toContain("STOP file");
    expect(haltText({ reason: "env_var" })).toContain("JARVIS_AUTONOMY");
    expect(haltText({ reason: "disabled" })).toContain("off");
    expect(haltText({ reason: "too_soon", wait_secs: 45 })).toContain("45s");
  });

  it("explains being busy as deferral, not as a failure", () => {
    // The user is mid-conversation. "Error" would be wrong; nothing is broken.
    const text = haltText({ reason: "busy", wait_secs: 90 });
    expect(text).toContain("90s");
    expect(text).toContain("waits");
    expect(text.toLowerCase()).not.toContain("error");
  });
});

describe("ago", () => {
  const now = 1_700_000_000_000; // fixed clock, so these never flake

  it("uses the coarsest unit that still says something", () => {
    expect(ago(now / 1000 - 5, now)).toBe("5s ago");
    expect(ago(now / 1000 - 59, now)).toBe("59s ago");
    expect(ago(now / 1000 - 60, now)).toBe("1 min ago");
    expect(ago(now / 1000 - 3599, now)).toBe("59 min ago");
    expect(ago(now / 1000 - 7200, now)).toBe("2h ago");
  });

  it("never reports a negative age when the clock disagrees", () => {
    // Stamps come from the Rust side; a small clock skew must not render
    // "-3s ago", which reads as a bug.
    expect(ago(now / 1000 + 30, now)).toBe("0s ago");
  });
});

describe("beatText", () => {
  const now = 1_700_000_000_000;
  const at = now / 1000 - 120;

  it("distinguishes never-beaten from beaten-and-idle", () => {
    // These look identical if you only check for "nothing happened", and they
    // mean very different things: one is a broken loop, one is a healthy one.
    expect(beatText(null, now)).toBe("waiting for the first beat");
    expect(beatText({ at, beat: { outcome: "idle" } }, now)).toBe(
      "checked 2 min ago · nothing to do",
    );
  });

  it("reports why a beat was held", () => {
    const text = beatText(
      {
        at,
        beat: { outcome: "held", halt: { reason: "busy", wait_secs: 30 } },
      },
      now,
    );
    expect(text).toContain("held 2 min ago");
    expect(text).toContain("30s");
  });

  it("counts actions with correct grammar", () => {
    expect(
      beatText({ at, beat: { outcome: "ran", actions: 1 } }, now),
    ).toContain("1 action");
    expect(
      beatText({ at, beat: { outcome: "ran", actions: 3 } }, now),
    ).toContain("3 actions");
  });
});
