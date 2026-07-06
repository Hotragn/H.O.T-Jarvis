import { describe, expect, it } from "vitest";
import { nextTheme, resolveInitialTheme } from "./theme";

describe("resolveInitialTheme", () => {
  it("honors a stored choice over the OS preference", () => {
    expect(resolveInitialTheme("light", true)).toBe("light");
    expect(resolveInitialTheme("dark", false)).toBe("dark");
  });

  it("falls back to the OS preference when nothing is stored", () => {
    expect(resolveInitialTheme(null, true)).toBe("dark");
    expect(resolveInitialTheme(null, false)).toBe("light");
  });

  it("ignores corrupted stored values", () => {
    expect(resolveInitialTheme("neon", true)).toBe("dark");
    expect(resolveInitialTheme("", false)).toBe("light");
  });
});

describe("nextTheme", () => {
  it("toggles between the two themes", () => {
    expect(nextTheme("dark")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
  });
});
