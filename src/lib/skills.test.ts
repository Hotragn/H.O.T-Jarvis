import { describe, expect, it } from "vitest";
import { testBadge } from "./skills";

describe("testBadge", () => {
  it("labels both statuses", () => {
    expect(testBadge({ status: "passed" })).toBe("✓ passed");
    expect(testBadge({ status: "failed", detail: "boom" })).toBe("✗ flagged");
  });
});
