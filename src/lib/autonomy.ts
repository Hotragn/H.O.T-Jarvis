import type { Halt, LastBeat } from "./ipc";

/// Plain-English reason the loop is halted. Never a bare enum: if auto mode
/// isn't running, the user deserves to know exactly why.
export function haltText(halt: Halt): string {
  switch (halt.reason) {
    case "stop_file":
      return "Halted by the STOP file. Release it to rearm.";
    case "env_var":
      return "Halted by the JARVIS_AUTONOMY environment variable.";
    case "disabled":
      return "Auto mode is off.";
    case "too_soon":
      return `Rate-limited — next cycle in ${halt.wait_secs}s.`;
    case "already_running":
      return "A cycle is already running.";
    case "busy":
      return `You're using the app — unattended work waits ${halt.wait_secs}s.`;
  }
}

/// How long ago, in words. Exact seconds are noise here; "3 min ago" is what you
/// actually want to know about a background loop.
export function ago(at: number, nowMs: number = Date.now()): string {
  const secs = Math.max(0, Math.floor(nowMs / 1000) - at);
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)} min ago`;
  return `${Math.floor(secs / 3600)}h ago`;
}

/// The heartbeat's last wake-up, in one line. A background loop you can't see is
/// indistinguishable from one that's broken, so this is never hidden.
export function beatText(
  last: LastBeat | null,
  nowMs: number = Date.now(),
): string {
  if (!last) return "waiting for the first beat";
  switch (last.beat.outcome) {
    case "held":
      return `held ${ago(last.at, nowMs)} · ${haltText(last.beat.halt).toLowerCase()}`;
    case "refused":
      return `held ${ago(last.at, nowMs)} · ${last.beat.why}`;
    case "idle":
      return `checked ${ago(last.at, nowMs)} · nothing to do`;
    case "ran":
      return `ran ${ago(last.at, nowMs)} · ${last.beat.actions} action${
        last.beat.actions === 1 ? "" : "s"
      }`;
  }
}
