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

/// How long to let the barge-in watcher run for a given answer.
///
/// The watcher holds the microphone open, so it must never outlive the playback
/// it is guarding. There is no API that reports how long an utterance will take,
/// so this estimates from length at a deliberately generous speaking rate and
/// adds slack: overshooting a little means the mic stays open a few seconds too
/// long, while undershooting means the tail of a long answer can't be
/// interrupted, which is exactly when you most want to.
export function speechBudgetMs(text: string): number {
  // Measured on the sanitized text, which is what actually gets spoken — and
  // which sanitizeForSpeech already caps, so the result is inherently bounded
  // without a second clamp. An explicit Math.min here would be unreachable code
  // pretending to be a safety net.
  const words = sanitizeForSpeech(text).split(/\s+/).filter(Boolean).length;
  // ~2.5 words/second is slower than any TTS default, so this over-estimates.
  const estimated = (words / 2.5) * 1000;
  return Math.max(estimated + 3_000, 4_000);
}

export function stopSpeaking(): void {
  if (ttsAvailable) window.speechSynthesis.cancel();
}

export const STT_UNAVAILABLE_MESSAGE =
  "Voice input isn't available in this window: the Windows WebView doesn't ship a speech recognizer, and this build has no local speech model. Rebuild with `--features local-whisper` for fully on-device dictation — voice replies already work.";

// ---- Voice v1 routing (pure; unit-tested) ----

/// Which engine a mic press should use.
/// - `local`: Whisper on this machine (preferred — private, offline, and the
///   only option that works inside WebView2).
/// - `download`: local engine is compiled in but the model isn't cached yet.
/// - `web`: fall back to the WebView's own recognizer where it exists.
/// - `none`: nothing can hear; say so honestly.
export type SttRoute = "local" | "download" | "web" | "none";

/// Deliberately prefers local over the Web Speech API even when both exist:
/// the browser recognizer streams audio to a cloud service on most platforms,
/// which breaks the promise that nothing leaves the machine.
export function chooseSttRoute(
  readiness: { state: string } | null,
  webRecognizerAvailable: boolean,
): SttRoute {
  switch (readiness?.state) {
    case "ready":
      return "local";
    case "needs_download":
      return "download";
    default:
      return webRecognizerAvailable ? "web" : "none";
  }
}

/// One-line status for the mic button's tooltip.
export function sttHint(route: SttRoute, model?: string, approxMb?: number): string {
  switch (route) {
    case "local":
      return `push to talk — ${model ?? "Whisper"} running on this machine`;
    case "download":
      return `click to download the speech model (~${approxMb ?? 43} MB, one time)`;
    case "web":
      return "push to talk — using this window's recognizer";
    case "none":
      return "voice input not available in this build";
  }
}
