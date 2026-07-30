// Undo affordances (§5.4). Pure, tested.

const REVERSIBLE = new Set([
  "chat.user",
  "chat.assistant",
  "note.saved",
  "note.deleted",
  "skill.saved",
  "skill.authored",
  "memory.reflected",
]);

export function isReversible(kind: string): boolean {
  return REVERSIBLE.has(kind);
}
