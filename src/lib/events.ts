// Turns raw log events into one-line timeline summaries. Pure, tested.

import type { AppEvent } from "./ipc";

function truncate(text: string, max = 96): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

export function summarizeEvent(event: AppEvent): string {
  const p = event.payload ?? {};
  switch (event.kind) {
    case "app.started":
      return `session started · v${str(p.version) || "?"}`;
    case "chat.user":
      return truncate(str(p.text));
    case "chat.assistant": {
      const route = [str(p.provider), str(p.model)].filter(Boolean).join(" · ");
      const ms = typeof p.duration_ms === "number" ? ` · ${p.duration_ms}ms` : "";
      return `${truncate(str(p.text))}  [${route}${ms}]`;
    }
    case "chat.failed":
      return truncate(`failed: ${str(p.error)}`);
    case "note.saved":
      return `saved note "${str(p.slug)}"`;
    case "memory.wiped":
      return "all remembered messages and facts erased";
    default:
      return truncate(JSON.stringify(p));
  }
}

/** Everything before the first dot: the actor/domain column of the timeline. */
export function eventDomain(kind: string): string {
  const dot = kind.indexOf(".");
  return dot === -1 ? kind : kind.slice(0, dot);
}
