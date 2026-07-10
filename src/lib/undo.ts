// Undo affordances (§5.4). Pure, tested.

const REVERSIBLE = new Set([
  "chat.user",
  "chat.assistant",
  "note.saved",
  "skill.saved",
  "skill.authored",
]);

export function isReversible(kind: string): boolean {
  return REVERSIBLE.has(kind);
}
