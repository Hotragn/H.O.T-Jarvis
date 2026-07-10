import { describe, expect, it } from "vitest";
import { confidenceLabel, confidenceTone } from "./confidence";

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
