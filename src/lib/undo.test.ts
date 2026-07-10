import { describe, expect, it } from "vitest";
import { isReversible } from "./undo";

describe("isReversible", () => {
  it("marks user-facing actions reversible", () => {
    expect(isReversible("chat.user")).toBe(true);
    expect(isReversible("chat.assistant")).toBe(true);
    expect(isReversible("note.saved")).toBe(true);
    expect(isReversible("skill.saved")).toBe(true);
    expect(isReversible("skill.authored")).toBe(true);
  });

  it("marks permanent actions irreversible", () => {
    expect(isReversible("memory.wiped")).toBe(false);
    expect(isReversible("memory.reflected")).toBe(false);
    expect(isReversible("app.started")).toBe(false);
    expect(isReversible("undo.chat")).toBe(false);
  });
});
