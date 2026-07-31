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
  /// Row id of the stored reply; needed to grade it (calibration).
  msg_id: number | null;
  /// Confidence v2: the stated number re-read through the calibration record.
  trust: Trust | null;
}

/// How much an answer is actually worth, after applying measured bias.
export interface Trust {
  raw: number;
  adjusted: number;
  /// Calibrated confidence is below the ask threshold.
  verify: boolean;
  /// The raw number would have passed but the calibrated one didn't — the
  /// case worth explaining to the user.
  demoted: boolean;
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

export interface ReplayedMessage {
  role: string;
  content: string;
}

export interface ReplayReport {
  matched: number;
  missing_in_db: ReplayedMessage[];
  extra_in_db: ReplayedMessage[];
  deterministic: boolean;
}

export async function undoEvent(eventId: number): Promise<string> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<string>("undo_event", { eventId });
}

// --- Replay v2: step-through player + state audit ---

export interface ReplayStep {
  step: number;
  event_id: number;
  kind: string;
  summary: string;
  changed: boolean;
  messages: number;
  notes: number;
  skills: number;
  insights: number;
}

export interface ReplayState {
  messages: ReplayedMessage[];
  notes: Record<string, number>;
  skills: Record<string, number>;
  insights: number[];
}

export interface KeyedDrift {
  missing: string[];
  extra: string[];
  differing: string[];
}

export interface StateReport {
  messages: ReplayReport;
  notes: KeyedDrift;
  skills: KeyedDrift;
  deterministic: boolean;
  summary: string;
}

export async function replayTimeline(): Promise<ReplayStep[]> {
  if (!inTauri) return [];
  return invoke<ReplayStep[]>("replay_timeline");
}

export async function replayStateAt(steps: number): Promise<ReplayState | null> {
  if (!inTauri) return null;
  return invoke<ReplayState>("replay_state_at", { steps });
}

export async function replayAuditState(): Promise<StateReport | null> {
  if (!inTauri) return null;
  return invoke<StateReport>("replay_audit_state");
}

export async function replayAudit(): Promise<ReplayReport> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<ReplayReport>("replay_audit");
}

export async function listNotes(): Promise<string[]> {
  if (!inTauri) return [];
  return invoke<string[]>("list_notes");
}

export async function readNote(name: string): Promise<string> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<string>("read_note", { name });
}

export async function deleteNote(name: string): Promise<void> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<void>("delete_note", { name });
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
  /// Times a later reflection re-derived this lesson (Reflection v1).
  corroborations: number;
  /// Times it has been injected into a prompt.
  uses: number;
}

export interface ForgetMerge {
  keep_id: number;
  drop_id: number;
  similarity: number;
}

/// What a maintenance pass did: duplicates collapsed, spent lessons dropped.
export interface ForgetPlan {
  merges: ForgetMerge[];
  forget: number[];
  reasons: [number, string][];
  kept: number;
}

export async function maintainInsights(): Promise<ForgetPlan | null> {
  if (!inTauri) return null;
  return invoke<ForgetPlan>("maintain_insights");
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

// --- semantic memory: meaning-based search over everything remembered ---

export interface SearchHit {
  id: number;
  role: string;
  content: string;
  created_at: number;
  /// Cosine similarity; show as a percentage.
  score: number;
}

export async function searchMemory(query: string, limit = 10): Promise<SearchHit[]> {
  if (!inTauri) return [];
  return invoke<SearchHit[]>("search_memory", { query, limit });
}

/// Backfills embeddings for old messages. Returns [indexed, remaining].
export async function indexMemory(): Promise<[number, number]> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<[number, number]>("index_memory");
}

// --- provider settings (custom models; the iOS companion enabler) ---

export interface ProviderSettings {
  ollama_base_url: string;
  ollama_model: string;
  groq_api_key: string;
  groq_model: string;
  openrouter_api_key: string;
  openrouter_model: string;
}

export async function getProviderSettings(): Promise<ProviderSettings | null> {
  if (!inTauri) return null;
  return invoke<ProviderSettings>("get_provider_settings");
}

/// Saves and applies immediately; resolves to whether the local endpoint is
/// now reachable (companion status).
export async function setProviderSettings(s: ProviderSettings): Promise<boolean> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<boolean>("set_provider_settings", {
    ollamaBaseUrl: s.ollama_base_url,
    ollamaModel: s.ollama_model,
    groqApiKey: s.groq_api_key,
    groqModel: s.groq_model,
    openrouterApiKey: s.openrouter_api_key,
    openrouterModel: s.openrouter_model,
  });
}

// --- M3: auto mode (§7) ---

export type AutonomyAction =
  | "reflect"
  | "tidy_insights"
  | "index_memory"
  | "test_skills"
  | "replay_audit"
  | "save_note"
  | "author_skill"
  | "run_skill"
  | "wipe_memory"
  | "delete_note"
  | "change_settings"
  | "external_side_effect";

export type Clearance = "auto" | "needs_approval" | "forbidden";

export interface AutonomyCaps {
  max_actions: number;
  max_tool_calls: number;
  max_seconds: number;
  min_cycle_gap_secs: number;
}

export type Halt =
  | { reason: "stop_file" }
  | { reason: "env_var" }
  | { reason: "disabled" }
  | { reason: "too_soon"; wait_secs: number };

export interface AutonomyStatus {
  enabled: boolean;
  caps: AutonomyCaps;
  /// null when a cycle could run right now.
  halt: Halt | null;
  stop_file: string;
  stop_file_exists: boolean;
  last_cycle_at: number | null;
}

export interface PlannedAction {
  action: AutonomyAction;
  clearance: Clearance;
  tool_calls: number;
  reason: string;
}

export interface CyclePlan {
  actions: PlannedAction[];
  deferred: PlannedAction[];
  stop_reason: string;
  caps: AutonomyCaps;
  idle: boolean;
}

export interface CycleResult {
  did: {
    action: AutonomyAction;
    reason: string;
    result: unknown;
    error: string | null;
  }[];
  usage: { actions: number; tool_calls: number; seconds: number };
  stop_reason: string;
  deferred: PlannedAction[];
}

export async function autonomyState(): Promise<AutonomyStatus | null> {
  if (!inTauri) return null;
  return invoke<AutonomyStatus>("autonomy_state");
}

export async function autonomySetEnabled(enabled: boolean): Promise<AutonomyStatus> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<AutonomyStatus>("autonomy_set_enabled", { enabled });
}

export async function autonomySetCaps(caps: AutonomyCaps): Promise<AutonomyStatus> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<AutonomyStatus>("autonomy_set_caps", { caps });
}

/// Engages or releases the emergency stop file.
export async function autonomyStopFile(engage: boolean): Promise<AutonomyStatus> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<AutonomyStatus>("autonomy_stop_file", { engage });
}

/// Dry run — what a cycle would do. Available even while halted.
export async function autonomyPlan(): Promise<CyclePlan | null> {
  if (!inTauri) return null;
  return invoke<CyclePlan>("autonomy_plan");
}

export async function autonomyRunCycle(): Promise<CycleResult> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<CycleResult>("autonomy_run_cycle");
}

// --- Voice v2: wake word + hands-free conversation ---

export type VoicePhase =
  | "off"
  | "waiting"
  | "listening"
  | "thinking"
  | "speaking"
  | "follow_up";

export interface VoiceSession {
  phase: VoicePhase;
  wake_phrase: string;
  /// Whether the mic should be open right now. The UI mirrors this rather than
  /// deciding for itself — the policy lives in the tested Rust core.
  wants_audio: boolean;
  needs_wake: boolean;
  follow_up_remaining_ms: number;
}

export type VoiceAction =
  | { action: "idle" }
  | { action: "ask"; text: string }
  | { action: "say"; text: string }
  | { action: "sleep" };

export interface VoiceHeard {
  action: VoiceAction;
  session: VoiceSession;
}

export async function voiceSession(): Promise<VoiceSession | null> {
  if (!inTauri) return null;
  return invoke<VoiceSession>("voice_session");
}

export async function voiceHandsFree(on: boolean): Promise<VoiceSession> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<VoiceSession>("voice_hands_free", { on });
}

export async function voiceSetWakePhrase(phrase: string): Promise<VoiceSession> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<VoiceSession>("voice_set_wake_phrase", { phrase });
}

export async function voiceHeard(transcript: string, durationMs: number): Promise<VoiceHeard> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<VoiceHeard>("voice_heard", { transcript, durationMs });
}

export type VoiceEvent =
  | "answered_speaking"
  | "answered_silent"
  | "finished_speaking"
  | "failed"
  | "tick";

export async function voiceAdvance(
  event: VoiceEvent,
  elapsedMs?: number,
): Promise<VoiceSession> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<VoiceSession>("voice_advance", { event, elapsedMs });
}

// --- Confidence v1: calibration tracking ---

export interface CalibrationBin {
  low: number;
  high: number;
  count: number;
  mean_confidence: number;
  accuracy: number;
}

export interface CalibrationReport {
  sample_size: number;
  brier: number;
  ece: number;
  mean_confidence: number;
  accuracy: number;
  /// Positive means overconfident.
  bias: number;
  bins: CalibrationBin[];
  trustworthy: boolean;
  summary: string;
}

export async function rateMessage(msgId: number, helpful: boolean): Promise<void> {
  if (!inTauri) return;
  return invoke<void>("rate_message", { msgId, helpful });
}

export async function calibrationReport(): Promise<CalibrationReport | null> {
  if (!inTauri) return null;
  return invoke<CalibrationReport>("calibration_report");
}

// --- Voice v1: on-device speech-to-text ---

/// Mirrors core::stt::SttReadiness. `not_compiled` means the build has no local
/// model, so the UI should say so rather than offer a dead button.
export type SttReadiness =
  | { state: "ready"; model: string }
  | { state: "needs_download"; model: string; approx_mb: number }
  | { state: "not_compiled" };

export async function sttStatus(): Promise<SttReadiness> {
  if (!inTauri) return { state: "not_compiled" };
  return invoke<SttReadiness>("stt_status");
}

export async function sttDevice(): Promise<string | null> {
  if (!inTauri) return null;
  return invoke<string | null>("stt_device");
}

export async function sttDownloadModel(): Promise<string> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<string>("stt_download_model");
}

export interface SttFormat {
  sample_rate: number;
  channels: number;
}

export async function sttStart(): Promise<SttFormat> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<SttFormat>("stt_start");
}

/// Returns the transcript, or an empty string when nothing usable was said.
export async function sttStop(): Promise<string> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<string>("stt_stop");
}

export interface SttHeard {
  transcript: string;
  duration_ms: number;
}

/// Hands-free capture: listens until the speaker stops, then transcribes.
/// An empty transcript means the window passed in silence — loop again.
export async function sttListen(
  maxWaitMs = 6000,
  maxUtteranceMs = 15000,
): Promise<SttHeard> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<SttHeard>("stt_listen", { maxWaitMs, maxUtteranceMs });
}

export async function sttCancel(): Promise<void> {
  if (!inTauri) return;
  return invoke<void>("stt_cancel");
}

export async function exportMemory(): Promise<unknown> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<unknown>("export_memory");
}

export async function wipeMemory(): Promise<void> {
  if (!inTauri) throw new Error("No backend in the browser preview.");
  return invoke<void>("wipe_memory");
}
