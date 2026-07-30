// Hands-free presentation helpers (§6.4, Voice v2). Pure and tested; the state
// machine itself lives in the Rust core so the policy has one home.

import type { VoicePhase, VoiceSession } from "./ipc";

/// What the user should see for each phase. Deliberately plain language: the
/// point of a hands-free indicator is that a glance tells you whether the
/// machine is listening, which is a privacy question as much as a UX one.
export function phaseLabel(phase: VoicePhase): string {
  switch (phase) {
    case "off":
      return "hands-free off";
    case "waiting":
      return "waiting for the wake phrase";
    case "listening":
      return "listening";
    case "thinking":
      return "thinking";
    case "speaking":
      return "speaking";
    case "follow_up":
      return "listening for a follow-up";
  }
}

/// Short badge text for the header toggle.
export function phaseBadge(phase: VoicePhase): string {
  switch (phase) {
    case "off":
      return "hands-free";
    case "waiting":
      return "armed";
    case "listening":
    case "follow_up":
      return "listening";
    case "thinking":
      return "thinking";
    case "speaking":
      return "speaking";
  }
}

/// True when the mic is genuinely capturing, so the UI can show an unambiguous
/// indicator. Mirrors the core rather than re-deriving it.
export function isCapturing(session: VoiceSession | null): boolean {
  return !!session && session.wants_audio;
}

/// Countdown for the follow-up window, in whole seconds. Returns null outside
/// the window so the UI shows nothing rather than "0s".
export function followUpSeconds(session: VoiceSession | null): number | null {
  if (!session || session.phase !== "follow_up") return null;
  const secs = Math.ceil(session.follow_up_remaining_ms / 1000);
  return secs > 0 ? secs : null;
}

/// The one-line hint under the toggle: what to say next.
export function nextHint(session: VoiceSession | null): string | null {
  if (!session || session.phase === "off") return null;
  if (session.needs_wake) return `Say "${session.wake_phrase}" to start.`;
  if (session.phase === "listening") return "Go ahead — I'm listening.";
  if (session.phase === "follow_up") {
    const secs = followUpSeconds(session);
    return secs
      ? `Ask a follow-up (${secs}s) — no wake phrase needed.`
      : "Ask a follow-up — no wake phrase needed.";
  }
  return null;
}

/// A wake phrase must be at least two words, mirroring the backend rule so the
/// user gets told before a round trip.
export function wakePhraseError(phrase: string): string | null {
  const words = phrase.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "Enter a wake phrase.";
  if (words.length < 2) {
    return "Use at least two words, so it doesn't trigger by accident.";
  }
  return null;
}
