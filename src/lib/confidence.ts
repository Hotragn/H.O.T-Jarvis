// Confidence display helpers (§5.3). Pure, tested.

export type ConfidenceTone = "high" | "mid" | "low";

export function confidenceTone(value: number): ConfidenceTone {
  if (value >= 70) return "high";
  if (value >= 40) return "mid";
  return "low";
}

export function confidenceLabel(value: number | null | undefined): string | null {
  if (value === null || value === undefined) return null;
  return `conf ${Math.round(value)}%`;
}

// --- Confidence v2: show the calibrated number, not just the claim ---

export interface TrustLike {
  raw: number;
  adjusted: number;
  verify: boolean;
  demoted: boolean;
}

/// The per-message label. Once calibration has evidence and disagrees with the
/// claim, show both — the claim alone would be misleading.
export function trustLabel(trust: TrustLike | null | undefined): string | null {
  if (!trust) return null;
  if (trust.adjusted === trust.raw) return `conf ${trust.raw}%`;
  return `conf ${trust.raw}% → ${trust.adjusted}% calibrated`;
}

/// The tone the badge should use: the calibrated value is the honest one.
export function trustTone(trust: TrustLike | null | undefined): ConfidenceTone | null {
  if (!trust) return null;
  return confidenceTone(trust.adjusted);
}

/// A short warning shown only when calibration demoted an answer below the
/// ask threshold — i.e. it reads confident but the record says verify it.
export function trustWarning(trust: TrustLike | null | undefined): string | null {
  if (!trust || !trust.demoted) return null;
  return `Reads confident (${trust.raw}%) but this model's track record puts it nearer ${trust.adjusted}% — worth verifying.`;
}
