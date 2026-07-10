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
