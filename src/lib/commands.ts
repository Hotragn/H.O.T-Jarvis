// Command-palette model. Pure and DOM-free so the ranking is testable.

export interface PaletteCommand {
  id: string;
  label: string;
  hint?: string; // e.g. shortcut or section
}

// Subsequence match ("mem" hits "MEMORY", "ntn" hits "new note"), scored by
// how early and how tightly the query letters land in the label.
export function scoreCommand(query: string, label: string): number | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  const l = label.toLowerCase();
  if (q.length === 0) return 0;
  let li = 0;
  let score = 0;
  let lastHit = -1;
  for (const ch of q) {
    const found = l.indexOf(ch, li);
    if (found === -1) return null;
    score += found - (lastHit + 1); // gaps cost points
    lastHit = found;
    li = found + 1;
  }
  return score + l.length * 0.01; // gentle tiebreak toward shorter labels
}

export function filterCommands(
  query: string,
  commands: PaletteCommand[],
): PaletteCommand[] {
  return commands
    .map((c) => ({ c, s: scoreCommand(query, c.label) }))
    .filter((x): x is { c: PaletteCommand; s: number } => x.s !== null)
    .sort((a, b) => a.s - b.s)
    .map((x) => x.c);
}
