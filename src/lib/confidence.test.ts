import { describe, expect, it } from "vitest";
import { confidenceLabel, confidenceTone, trustLabel, trustTone, trustWarning } from "./confidence";

describe("confidenceTone", () => {
  it("buckets high, mid, and low", () => {
    expect(confidenceTone(92)).toBe("high");
    expect(confidenceTone(70)).toBe("high");
    expect(confidenceTone(55)).toBe("mid");
    expect(confidenceTone(40)).toBe("mid");
    expect(confidenceTone(39)).toBe("low");
    expect(confidenceTone(0)).toBe("low");
  });
});

describe("confidenceLabel", () => {
  it("formats a value and passes through absence", () => {
    expect(confidenceLabel(78)).toBe("conf 78%");
    expect(confidenceLabel(null)).toBeNull();
    expect(confidenceLabel(undefined)).toBeNull();
  });
});

describe("Confidence v2 — calibrated display", () => {
  const t = (raw: number, adjusted: number, demoted = false) => ({
    raw,
    adjusted,
    verify: adjusted < 40,
    demoted,
  });

  it("shows one number when calibration agrees with the claim", () => {
    expect(trustLabel(t(80, 80))).toBe("conf 80%");
  });

  it("shows both numbers once calibration disagrees", () => {
    // Hiding the correction would leave the misleading claim on screen.
    expect(trustLabel(t(80, 50))).toBe("conf 80% → 50% calibrated");
  });

  it("tones the badge by the calibrated value, not the claim", () => {
    // Claims 'high' but calibrates to 'low' — the badge must not say high.
    expect(trustTone(t(85, 30))).toBe("low");
    expect(trustTone(t(85, 85))).toBe("high");
  });

  it("warns only when an answer was actually demoted", () => {
    expect(trustWarning(t(60, 30, true))).toContain("worth verifying");
    expect(trustWarning(t(60, 30, true))).toContain("60%");
    expect(trustWarning(t(20, 10, false))).toBeNull();
    expect(trustWarning(t(90, 90))).toBeNull();
  });

  it("passes through a missing trust value", () => {
    expect(trustLabel(null)).toBeNull();
    expect(trustTone(undefined)).toBeNull();
    expect(trustWarning(null)).toBeNull();
  });
});
