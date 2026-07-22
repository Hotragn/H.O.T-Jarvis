// Voice (§6.4), v0: speech synthesis via the OS voices already on the
// machine (free, offline, no downloads) and push-to-talk recognition where
// the platform provides it. Fully optional; everything degrades gracefully.
// Pure decision helpers live here and are unit-tested; the thin DOM glue
// stays at the bottom.

export interface VoiceLike {
  name: string;
  lang: string;
}

/// Prefers a natural-sounding English voice, then any English voice, then
/// whatever the platform has. Deterministic given the same list.
export function pickVoice<V extends VoiceLike>(voices: V[]): V | null {
  if (voices.length === 0) return null;
  const english = voices.filter((v) => v.lang.toLowerCase().startsWith("en"));
  const natural = english.find((v) => /natural|neural/i.test(v.name));
  return natural ?? english[0] ?? voices[0];
}

/// Text is written for reading; speech needs a lighter cut. Code blocks are
/// summarized instead of read character by character; markdown noise and
/// URLs are stripped; length is capped so a long answer doesn't hold the
/// room hostage.
export function sanitizeForSpeech(text: string, maxChars = 600): string {
  let out = text
    .replace(/```[\s\S]*?```/g, " …code omitted… ")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/https?:\/\/\S+/g, " a link ")
    .replace(/[*_#>|]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (out.length > maxChars) {
    const cut = out.slice(0, maxChars);
    const lastStop = Math.max(cut.lastIndexOf(". "), cut.lastIndexOf("! "), cut.lastIndexOf("? "));
    out = lastStop > maxChars / 2 ? cut.slice(0, lastStop + 1) : `${cut}…`;
  }
  return out;
}

// ---- DOM glue (not unit-tested; exercised in the running app) ----

export const ttsAvailable =
  typeof window !== "undefined" && "speechSynthesis" in window;

// WebView2 (Tauri on Windows) ships speechSynthesis but usually not
// SpeechRecognition — feature-detect instead of assuming.
type RecognitionCtor = new () => {
  lang: string;
  interimResults: boolean;
  maxAlternatives: number;
  onresult: ((e: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null;
  onend: (() => void) | null;
  onerror: ((e: { error: string }) => void) | null;
  start: () => void;
  stop: () => void;
};

export function recognitionCtor(): RecognitionCtor | null {
  if (typeof window === "undefined") return null;
  const w = window as unknown as Record<string, unknown>;
  return (w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null) as RecognitionCtor | null;
}

export const sttAvailable = recognitionCtor() !== null;

export function speak(
  text: string,
  handlers: { onstart?: () => void; onend?: () => void } = {},
): void {
  if (!ttsAvailable) return;
  const spoken = sanitizeForSpeech(text);
  if (!spoken) return;
  window.speechSynthesis.cancel();
  const utterance = new SpeechSynthesisUtterance(spoken);
  const voice = pickVoice(window.speechSynthesis.getVoices());
  if (voice) utterance.voice = voice as SpeechSynthesisVoice;
  utterance.rate = 1.02;
  if (handlers.onstart) utterance.onstart = handlers.onstart;
  if (handlers.onend) {
    utterance.onend = handlers.onend;
    utterance.onerror = handlers.onend;
  }
  window.speechSynthesis.speak(utterance);
}

export function stopSpeaking(): void {
  if (ttsAvailable) window.speechSynthesis.cancel();
}

export const STT_UNAVAILABLE_MESSAGE =
  "Voice input isn't available in this window yet: the Windows WebView doesn't ship a speech recognizer. A fully local speech-to-text engine (Whisper, running on your machine) is on the roadmap — voice replies already work.";
