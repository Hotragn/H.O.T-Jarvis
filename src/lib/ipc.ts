// Typed bridge to the Rust core. Outside Tauri (plain-browser `npm run dev`)
// it degrades to an inert preview instead of crashing.

import { invoke } from "@tauri-apps/api/core";

export interface ProviderStatus {
  id: string;
  configured: boolean;
  reachable: boolean | null;
  model: string;
}

export interface Status {
  providers: ProviderStatus[];
  ready: boolean;
  onboarding: string | null;
  message_count: number;
  fact_count: number;
}

export interface StoredMessage {
  id: number;
  role: string;
  content: string;
  created_at: number;
}

export interface ChatReply {
  content: string;
  provider: string;
  model: string;
  cached: boolean;
  confidence: number | null;
}

export interface Telemetry {
  cpu_percent: number;
  mem_used: number;
  mem_total: number;
  uptime_secs: number;
  note_count: number;
  message_count: number;
  fact_count: number;
}

export interface AppEvent {
  id: number;
  ts: number;
  kind: string;
  payload: Record<string, unknown>;
}

export type TestStatus =
  | { status: "passed" }
  | { status: "failed"; detail: string };

export interface SkillManifest {
  name: string;
  version: number;
  description: string;
  created_at: number;
  updated_at: number;
  test_status: TestStatus;
}

export const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function getStatus(): Promise<Status> {
  if (!inTauri) {
    return {
      providers: [],
      ready: false,
      onboarding:
        "This is the browser preview — run `npm run tauri dev` to launch the real app with memory and models.",
      message_count: 0,
      fact_count: 0,
    };
  }
  return invoke<Status>("get_status");
}

export async function getHistory(limit = 200): Promise<StoredMessage[]> {
  if (!inTauri) return [];
  return invoke<StoredMessage[]>("get_history", { limit });
}

export async function chatSend(text: string): Promise<ChatReply> {
  if (!inTauri) {
    throw new Error(
      "No backend in the browser preview — launch with `npm run tauri dev`.",
    );
  }
  return invoke<ChatReply>("chat_send", { text });
}

export async function getTelemetry(): Promise<Telemetry | null> {
  if (!inTauri) return null;
  return invoke<Telemetry>("get_telemetry");
}

export async function getEvents(limit = 200): Promise<AppEvent[]> {
  if (!inTauri) return [];
  return invoke<AppEvent[]>("get_events", { limit });
}

export async function listNotes(): Promise<string[]> {
  if (!inTauri) return [];
  return invoke<string[]>("list_notes");
}

export async function readNote(name: string): Promise<string> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<string>("read_note", { name });
}

export async function saveNote(title: string, content: string): Promise<string> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<string>("save_note", { title, content });
}

export async function listSkills(): Promise<SkillManifest[]> {
  if (!inTauri) return [];
  return invoke<SkillManifest[]>("list_skills");
}

export async function saveSkill(
  name: string,
  description: string,
  code: string,
  test: string,
): Promise<SkillManifest> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<SkillManifest>("save_skill", { name, description, code, test });
}

export interface AuthoringOutcome {
  manifest: SkillManifest;
  attempts: number;
  passed: boolean;
}

export async function authorSkill(request: string): Promise<AuthoringOutcome> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<AuthoringOutcome>("author_skill", { request });
}

export async function testSkill(name: string): Promise<SkillManifest> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<SkillManifest>("test_skill", { name });
}

export async function runSkill(name: string, input: string): Promise<string> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<string>("run_skill", { name, input });
}

export interface Insight {
  id: number;
  kind: string;
  content: string;
  source: string;
  created_at: number;
}

export async function listInsights(limit = 50): Promise<Insight[]> {
  if (!inTauri) return [];
  return invoke<Insight[]>("list_insights", { limit });
}

export async function reflectNow(): Promise<Insight[]> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<Insight[]>("reflect_now");
}

export async function reflectIfDue(): Promise<number | null> {
  if (!inTauri) return null;
  return invoke<number | null>("reflect_if_due");
}

export async function exportMemory(): Promise<unknown> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<unknown>("export_memory");
}

export async function wipeMemory(): Promise<void> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<void>("wipe_memory");
}
