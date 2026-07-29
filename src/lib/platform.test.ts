import { describe, expect, it } from "vitest";
import { isIOS, isTouchDevice, showSystemTelemetry } from "./platform";

const IPHONE = {
  userAgent:
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15",
  maxTouchPoints: 5,
};
const IPAD_MASQUERADING = {
  // Modern iPadOS reports itself as a Mac; touch points give it away.
  userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
  maxTouchPoints: 5,
};
const REAL_MAC = {
  userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
  maxTouchPoints: 0,
};
const WINDOWS = {
  userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
  maxTouchPoints: 0,
};
const WINDOWS_TOUCH = {
  userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
  maxTouchPoints: 10,
};

describe("isIOS", () => {
  it("detects iPhone by user agent", () => {
    expect(isIOS(IPHONE)).toBe(true);
  });

  it("detects an iPad masquerading as a Mac via touch points", () => {
    expect(isIOS(IPAD_MASQUERADING)).toBe(true);
  });

  it("does not flag a real Mac or a Windows machine", () => {
    expect(isIOS(REAL_MAC)).toBe(false);
    expect(isIOS(WINDOWS)).toBe(false);
    // A Windows touch laptop is touch, but not iOS.
    expect(isIOS(WINDOWS_TOUCH)).toBe(false);
  });
});

describe("capability routing", () => {
  it("touch devices are detected regardless of OS", () => {
    expect(isTouchDevice(IPHONE)).toBe(true);
    expect(isTouchDevice(WINDOWS_TOUCH)).toBe(true);
    expect(isTouchDevice(WINDOWS)).toBe(false);
  });

  it("system telemetry hides on iOS, shows elsewhere", () => {
    // Sandboxed system stats on iOS are decoration pretending to be data.
    expect(showSystemTelemetry(IPHONE)).toBe(false);
    expect(showSystemTelemetry(IPAD_MASQUERADING)).toBe(false);
    expect(showSystemTelemetry(REAL_MAC)).toBe(true);
    expect(showSystemTelemetry(WINDOWS)).toBe(true);
  });
});
