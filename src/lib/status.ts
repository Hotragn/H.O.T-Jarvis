import type { Status } from "./ipc";

export interface StatusPill {
  label: string;
  tone: "ok" | "warn";
}

// Summarizes backend status into the header pill: which brain is answering.
export function describeStatus(status: Status | null): StatusPill {
  if (!status) return { label: "connecting…", tone: "warn" };
  const ollama = status.providers.find((p) => p.id === "ollama");
  if (ollama?.reachable) return { label: `local · ${ollama.model}`, tone: "ok" };
  const cloud = status.providers.find((p) => p.id !== "ollama" && p.configured);
  if (cloud) return { label: `${cloud.id} · ${cloud.model}`, tone: "ok" };
  return { label: "no model — free setup needed", tone: "warn" };
}
