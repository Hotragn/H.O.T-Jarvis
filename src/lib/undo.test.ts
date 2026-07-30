import { describe, expect, it } from "vitest";
import { isReversible } from "./undo";

describe("isReversible", () => {
  it("marks user-facing actions reversible", () => {
    expect(isReversible("chat.user")).toBe(true);
    expect(isReversible("chat.assistant")).toBe(true);
    expect(isReversible("note.saved")).toBe(true);
    expect(isReversible("note.deleted")).toBe(true);
    expect(isReversible("skill.saved")).toBe(true);
    expect(isReversible("skill.authored")).toBe(true);
  });

  it("marks a reflection pass reversible", () => {
    // Replay v2 changed this: a pass now logs the ids of the lessons it
    // created, so undoing it means dropping exactly those. Before that it was
    // only counted, and therefore genuinely irreversible.
    expect(isReversible("memory.reflected")).toBe(true);
  });

  it("marks genuinely permanent actions irreversible", () => {
    // A wipe destroys the data needed to reverse it, by design.
    expect(isReversible("memory.wiped")).toBe(false);
    // Forgetting records the reason, not the lesson text — nothing to restore.
    expect(isReversible("memory.forgot_insights")).toBe(false);
    // Observations, not actions.
    expect(isReversible("app.started")).toBe(false);
    expect(isReversible("voice.transcribed")).toBe(false);
    // Undos append to history rather than being undone themselves.
    expect(isReversible("undo.chat")).toBe(false);
    expect(isReversible("undo.reflection")).toBe(false);
  });
});
