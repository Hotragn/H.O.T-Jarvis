import { describe, expect, it } from "vitest";
import { filterCommands, scoreCommand, type PaletteCommand } from "./commands";

const commands: PaletteCommand[] = [
  { id: "tab-chat", label: "Go to chat" },
  { id: "tab-notes", label: "Go to notes" },
  { id: "tab-memory", label: "Go to memory" },
  { id: "new-note", label: "New note" },
  { id: "theme", label: "Toggle theme" },
  { id: "export", label: "Export memory" },
];

describe("scoreCommand", () => {
  it("matches subsequences and rejects non-matches", () => {
    expect(scoreCommand("mem", "Go to memory")).not.toBeNull();
    expect(scoreCommand("xyz", "Go to memory")).toBeNull();
  });

  it("empty query matches everything", () => {
    expect(scoreCommand("", "anything")).toBe(0);
  });
});

describe("filterCommands", () => {
  it("returns all commands for an empty query", () => {
    expect(filterCommands("", commands)).toHaveLength(commands.length);
  });

  it("ranks tight matches above loose ones", () => {
    const result = filterCommands("note", commands);
    expect(result[0].id).toBe("new-note");
    expect(result.map((c) => c.id)).toContain("tab-notes");
  });

  it("drops commands that do not match", () => {
    const result = filterCommands("theme", commands);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("theme");
  });
});
