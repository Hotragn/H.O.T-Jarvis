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
