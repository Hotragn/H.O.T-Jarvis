// Platform detection (pure, tested). The UI degrades honestly per platform:
// iOS has no meaningful system-wide CPU/RAM stats for a sandboxed app, no
// system tray, and no global hotkey — so those affordances hide rather than
// showing dead or misleading chrome.

export interface PlatformLike {
  userAgent: string;
  maxTouchPoints: number;
}

/// True on iPhone/iPad. Modern iPadOS masquerades as macOS in the UA, so the
/// touch-point count is the discriminator Apple themselves recommend.
export function isIOS(p: PlatformLike): boolean {
  if (/iPhone|iPad|iPod/i.test(p.userAgent)) return true;
  return /Macintosh/i.test(p.userAgent) && p.maxTouchPoints > 1;
}

/// Coarse-pointer devices get bigger touch targets and a 16px input font
/// (below 16px, iOS Safari auto-zooms the page on focus).
export function isTouchDevice(p: PlatformLike): boolean {
  return p.maxTouchPoints > 0;
}

/// Whether to show the desktop telemetry strip (CPU sparkline, RAM, uptime).
/// On iOS those numbers are sandboxed to meaninglessness; showing them would
/// be decoration pretending to be data.
export function showSystemTelemetry(p: PlatformLike): boolean {
  return !isIOS(p);
}

/// The live values for this session.
export const platform: PlatformLike =
  typeof navigator !== "undefined"
    ? { userAgent: navigator.userAgent, maxTouchPoints: navigator.maxTouchPoints ?? 0 }
    : { userAgent: "", maxTouchPoints: 0 };
