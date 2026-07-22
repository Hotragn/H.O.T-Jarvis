import { describe, expect, it } from "vitest";
import { pickVoice, sanitizeForSpeech } from "./voice";

describe("pickVoice", () => {
  const voice = (name: string, lang: string) => ({ name, lang });

  it("prefers natural or neural english voices", () => {
    const voices = [
      voice("Microsoft David", "en-US"),
      voice("Microsoft Aria Natural", "en-US"),
      voice("Hortense", "fr-FR"),
    ];
    expect(pickVoice(voices)?.name).toBe("Microsoft Aria Natural");
  });

  it("falls back to any english, then anything, then null", () => {
    expect(
      pickVoice([voice("Hortense", "fr-FR"), voice("David", "en-GB")])?.name,
    ).toBe("David");
    expect(pickVoice([voice("Hortense", "fr-FR")])?.name).toBe("Hortense");
    expect(pickVoice([])).toBeNull();
  });
});

describe("sanitizeForSpeech", () => {
  it("summarizes code blocks instead of reading them", () => {
    const text = "Here you go:\n```rust\nfn main() {}\n```\nDone.";
    const spoken = sanitizeForSpeech(text);
    expect(spoken).toContain("code omitted");
    expect(spoken).not.toContain("fn main");
  });

  it("strips markdown noise, urls, and inline backticks", () => {
    const spoken = sanitizeForSpeech(
      "**Bold** `inline` see https://example.com/x for more",
    );
    expect(spoken).toBe("Bold inline see a link for more");
  });

  it("caps long answers at a sentence boundary", () => {
    const long = `${"This is a sentence. ".repeat(60)}`;
    const spoken = sanitizeForSpeech(long);
    expect(spoken.length).toBeLessThanOrEqual(600);
    expect(spoken.endsWith(".")).toBe(true);
  });
});
