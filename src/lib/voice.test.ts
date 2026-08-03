import { describe, expect, it } from "vitest";
import {
  chooseSttRoute,
  pickVoice,
  sanitizeForSpeech,
  speechBudgetMs,
  sttHint,
} from "./voice";

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

describe("chooseSttRoute", () => {
  it("prefers the local model, even when the web recognizer exists", () => {
    // Privacy is the point: the browser recognizer usually round-trips to a
    // cloud service, so local wins whenever it's ready.
    expect(chooseSttRoute({ state: "ready" }, true)).toBe("local");
    expect(chooseSttRoute({ state: "ready" }, false)).toBe("local");
  });

  it("asks for the download when the engine is present but the model isn't", () => {
    expect(chooseSttRoute({ state: "needs_download" }, true)).toBe("download");
    expect(chooseSttRoute({ state: "needs_download" }, false)).toBe("download");
  });

  it("falls back to the web recognizer only when there is no local engine", () => {
    expect(chooseSttRoute({ state: "not_compiled" }, true)).toBe("web");
    expect(chooseSttRoute(null, true)).toBe("web");
  });

  it("reports nothing available rather than offering a dead button", () => {
    expect(chooseSttRoute({ state: "not_compiled" }, false)).toBe("none");
    expect(chooseSttRoute(null, false)).toBe("none");
  });
});

describe("sttHint", () => {
  it("names the local model and stresses that it stays on the machine", () => {
    const hint = sttHint("local", "tiny.en");
    expect(hint).toContain("tiny.en");
    expect(hint).toContain("this machine");
  });

  it("states the one-time download size up front", () => {
    expect(sttHint("download", "tiny.en", 43)).toContain("43 MB");
  });

  it("has an honest line for every route", () => {
    for (const route of ["local", "download", "web", "none"] as const) {
      expect(sttHint(route).length).toBeGreaterThan(0);
    }
  });
});

// Voice v3: the barge-in watcher holds the microphone open, so its budget must
// never outlive the playback it guards.
describe("speechBudgetMs", () => {
  it("scales with the length of the answer", () => {
    const short = speechBudgetMs("Yes.");
    const long = speechBudgetMs(Array(200).fill("word").join(" "));
    expect(long).toBeGreaterThan(short);
  });

  it("over-estimates rather than cutting playback short", () => {
    // Undershooting means the tail of a long answer can't be interrupted, which
    // is exactly when you most want to. 25 words at any real TTS rate is under
    // 10s, so the budget must comfortably exceed that.
    expect(speechBudgetMs(Array(25).fill("word").join(" "))).toBeGreaterThan(10_000);
  });

  it("has a floor, so an empty answer still gets a usable window", () => {
    expect(speechBudgetMs("")).toBeGreaterThanOrEqual(4_000);
  });

  it("is bounded because the spoken text is, not because of a clamp", () => {
    // This is the property that actually stops an unbounded open microphone:
    // sanitizeForSpeech truncates, so no input can produce an arbitrarily long
    // budget. Pinning the real ceiling means shipping a longer speech cap can't
    // silently uncap the watcher.
    const huge = speechBudgetMs(Array(100_000).fill("word").join(" "));
    const capped = speechBudgetMs(Array(200).fill("word").join(" "));
    expect(huge).toBe(capped);
    expect(huge).toBeLessThan(60_000);
  });

  it("ignores markup the way playback does", () => {
    // The budget has to match what is actually spoken, not the raw text: code
    // fences and asterisks are stripped before speaking.
    const plain = speechBudgetMs("hello there friend");
    expect(speechBudgetMs("**hello** *there* `friend`")).toBe(plain);
  });
});
