import { describe, expect, it } from "vitest";
import { asPercent, asScore, biasPoints, binGap, binLabel, verdict } from "./calibration";

describe("verdict", () => {
  it("stays unknown until there is enough evidence", () => {
    // Claiming calibration off a tiny sample would be the very overconfidence
    // this feature exists to catch.
    expect(verdict({ bias: 0.4, trustworthy: false })).toBe("unknown");
    expect(verdict({ bias: 0, trustworthy: false })).toBe("unknown");
  });

  it("names overconfidence and underconfidence by sign", () => {
    expect(verdict({ bias: 0.3, trustworthy: true })).toBe("overconfident");
    expect(verdict({ bias: -0.3, trustworthy: true })).toBe("underconfident");
  });

  it("treats a small gap as calibrated", () => {
    expect(verdict({ bias: 0.05, trustworthy: true })).toBe("calibrated");
    expect(verdict({ bias: -0.05, trustworthy: true })).toBe("calibrated");
    // Exactly at the threshold is not yet notable.
    expect(verdict({ bias: 0.1, trustworthy: true })).toBe("calibrated");
  });
});

describe("formatting", () => {
  it("renders bias as signed whole points", () => {
    expect(biasPoints({ bias: 0.234 })).toBe(23);
    expect(biasPoints({ bias: -0.156 })).toBe(-16);
    expect(biasPoints({ bias: 0 })).toBe(0);
  });

  it("renders rates as percentages and scores to two decimals", () => {
    expect(asPercent(0.9)).toBe("90%");
    expect(asPercent(0)).toBe("0%");
    expect(asScore(0.12345)).toBe("0.12");
    expect(asScore(1)).toBe("1.00");
  });

  it("labels bands and reports their gap from the diagonal", () => {
    expect(binLabel({ low: 90, high: 100 })).toBe("90-100");
    expect(binGap({ mean_confidence: 0.9, accuracy: 0.6 })).toBe(30);
    expect(binGap({ mean_confidence: 0.4, accuracy: 0.9 })).toBe(-50);
    expect(binGap({ mean_confidence: 0.7, accuracy: 0.7 })).toBe(0);
  });
});
