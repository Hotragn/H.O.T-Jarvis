import { describe, expect, it } from "vitest";
import { formatBytes, formatClock, formatDuration } from "./format";

describe("formatDuration", () => {
  it("renders seconds, minutes, hours tiers", () => {
    expect(formatDuration(42)).toBe("42s");
    expect(formatDuration(125)).toBe("2m 05s");
    expect(formatDuration(3660)).toBe("1h 01m");
  });

  it("clamps negatives to zero", () => {
    expect(formatDuration(-5)).toBe("0s");
  });
});

describe("formatBytes", () => {
  it("switches units sensibly", () => {
    expect(formatBytes(512 * 1024 ** 2)).toBe("512 MB");
    expect(formatBytes(8 * 1024 ** 3)).toBe("8.0 GB");
    expect(formatBytes(0)).toBe("0 MB");
  });
});

describe("formatClock", () => {
  it("pads to HH:MM:SS", () => {
    expect(formatClock(new Date(2026, 6, 6, 9, 5, 3))).toBe("09:05:03");
  });
});
