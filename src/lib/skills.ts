// Skill-library display helpers. Pure, tested.

import type { TestStatus } from "./ipc";

export function testBadge(status: TestStatus): string {
  return status.status === "passed" ? "✓ passed" : "✗ flagged";
}
