// Presentation helpers for the calibration panel (§5.3). Pure and unit-tested;
// the numbers themselves are computed in the Rust core.

import type { CalibrationBin, CalibrationReport } from "./ipc";

/// Signed bias in percentage points, rounded for display.
export function biasPoints(report: Pick<CalibrationReport, "bias">): number {
  return Math.round(report.bias * 100);
}

/// A one-word verdict for the badge. `unknown` until there's enough data —
/// claiming calibration off three answers would be exactly the overconfidence
/// this feature exists to catch.
export type CalibrationVerdict = "unknown" | "calibrated" | "overconfident" | "underconfident";

export function verdict(
  report: Pick<CalibrationReport, "bias" | "trustworthy">,
  notableBias = 0.1,
): CalibrationVerdict {
  if (!report.trustworthy) return "unknown";
  if (report.bias > notableBias) return "overconfident";
  if (report.bias < -notableBias) return "underconfident";
  return "calibrated";
}

/// Formats a 0-1 rate as a percentage with no decimals.
export function asPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/// Scores like Brier and ECE are read to two decimals; more is false precision.
export function asScore(value: number): string {
  return value.toFixed(2);
}

/// Label for a reliability band, e.g. "90-100".
export function binLabel(bin: Pick<CalibrationBin, "low" | "high">): string {
  return `${bin.low}-${bin.high}`;
}

/// How far a band sits off the diagonal, in points. Positive means the band
/// claimed more than it delivered.
export function binGap(bin: Pick<CalibrationBin, "mean_confidence" | "accuracy">): number {
  return Math.round((bin.mean_confidence - bin.accuracy) * 100);
}
