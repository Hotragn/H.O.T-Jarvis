import { useCallback, useEffect, useState } from "react";
import {
  autonomyPlan,
  autonomyRunCycle,
  autonomySetEnabled,
  autonomyState,
  autonomyStopFile,
  inTauri,
  type AutonomyStatus,
  type CyclePlan,
  type Halt,
} from "../lib/ipc";

/// Plain-English reason the loop is halted. Never a bare enum: if auto mode
/// isn't running, the user deserves to know exactly why.
function haltText(halt: Halt): string {
  switch (halt.reason) {
    case "stop_file":
      return "Halted by the STOP file. Release it to rearm.";
    case "env_var":
      return "Halted by the JARVIS_AUTONOMY environment variable.";
    case "disabled":
      return "Auto mode is off.";
    case "too_soon":
      return `Rate-limited — next cycle in ${halt.wait_secs}s.`;
  }
}

// Auto mode (§7). The guardrails are the feature, so the panel leads with them:
// what it may do, what it will never do without asking, and how to stop it.
export default function AutonomyPanel() {
  const [status, setStatus] = useState<AutonomyStatus | null>(null);
  const [plan, setPlan] = useState<CyclePlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(() => {
    autonomyState().then(setStatus).catch(() => {});
    autonomyPlan().then(setPlan).catch(() => {});
  }, []);

  useEffect(refresh, [refresh]);

  const guard = async (fn: () => Promise<AutonomyStatus>) => {
    setBusy(true);
    setNotice(null);
    try {
      setStatus(await fn());
      autonomyPlan().then(setPlan).catch(() => {});
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  };

  const runCycle = async () => {
    setBusy(true);
    setNotice(null);
    try {
      const result = await autonomyRunCycle();
      const ran = result.did.length;
      setNotice(
        ran === 0
          ? `Cycle finished with nothing to do (${result.stop_reason}).`
          : `Ran ${ran} action(s) in ${result.usage.seconds}s using ${result.usage.tool_calls} tool call(s). Stopped: ${result.stop_reason}.`,
      );
      refresh();
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!inTauri) {
    return (
      <section className="autonomy">
        <div className="panel-title-row">
          <span className="panel-title">auto mode</span>
        </div>
        <p className="panel-hint">
          Auto mode acts on the real app — launch it to arm or inspect the loop.
        </p>
      </section>
    );
  }

  const halted = !!status?.halt;

  return (
    <section className="autonomy" aria-label="auto mode">
      <div className="panel-title-row">
        <span className="panel-title">
          auto mode {status?.enabled ? "· armed" : "· off"}
        </span>
        <span className="editor-actions">
          <button
            type="button"
            className="ghost-btn"
            data-active={status?.enabled}
            disabled={busy}
            onClick={() => void guard(() => autonomySetEnabled(!status?.enabled))}
          >
            {status?.enabled ? "Disarm" : "Arm"}
          </button>
          <button
            type="button"
            className="ghost-btn danger"
            data-active={status?.stop_file_exists}
            disabled={busy}
            title="emergency stop — halts the loop regardless of any other setting"
            onClick={() => void guard(() => autonomyStopFile(!status?.stop_file_exists))}
          >
            {status?.stop_file_exists ? "Release STOP" : "STOP"}
          </button>
          <button
            type="button"
            className="ghost-btn"
            disabled={busy || halted}
            title={halted ? "halted — see the reason below" : "run one cycle now"}
            onClick={() => void runCycle()}
          >
            {busy ? "Running…" : "Run one cycle"}
          </button>
        </span>
      </div>

      <p className="panel-hint">
        Auto mode does only bounded self-maintenance: verifying the log, testing
        existing skills, indexing memory for recall, reflecting, and tidying
        lessons. It will never write notes, author or run skills, change
        settings, or wipe anything on its own — those need you.
      </p>

      {status?.halt && (
        <div className="msg" data-role="system">
          {haltText(status.halt)}
        </div>
      )}

      {notice && (
        <div className="msg" data-role="system">
          {notice}
        </div>
      )}

      {status && (
        <div className="autonomy-caps">
          <span>
            <b>{status.caps.max_actions}</b> actions
          </span>
          <span>
            <b>{status.caps.max_tool_calls}</b> tool calls
          </span>
          <span>
            <b>{status.caps.max_seconds}</b>s
          </span>
          <span>
            every <b>{Math.round(status.caps.min_cycle_gap_secs / 60)}</b> min
          </span>
        </div>
      )}

      {/* The dry run: what a cycle would do, before it does it. */}
      <div className="panel-title-row">
        <span className="panel-title">
          next cycle · dry run{plan?.idle ? " · nothing to do" : ""}
        </span>
        <button type="button" className="ghost-btn" disabled={busy} onClick={refresh}>
          Refresh
        </button>
      </div>

      {plan && plan.actions.length > 0 ? (
        <ul className="autonomy-plan">
          {plan.actions.map((a) => (
            <li key={a.action} className="autonomy-step">
              <span className="autonomy-step-name">{a.action.replace(/_/g, " ")}</span>
              <span className="autonomy-step-reason">{a.reason}</span>
              <span className="autonomy-step-cost">
                {a.tool_calls === 0 ? "local" : `${a.tool_calls} call`}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="panel-hint">
          Nothing worth doing right now — that's the normal state for a quiet app.
        </p>
      )}

      {plan && plan.deferred.length > 0 && (
        <>
          <div className="panel-title-row">
            <span className="panel-title">waiting for you · {plan.deferred.length}</span>
          </div>
          <ul className="autonomy-plan">
            {plan.deferred.map((a) => (
              <li key={a.action} className="autonomy-step" data-deferred="true">
                <span className="autonomy-step-name">{a.action.replace(/_/g, " ")}</span>
                <span className="autonomy-step-reason">{a.reason}</span>
                <span className="autonomy-step-cost">needs you</span>
              </li>
            ))}
          </ul>
        </>
      )}

      {status && (
        <p className="settings-note">
          Emergency stop file: <code>{status.stop_file}</code> — create it by hand
          and the loop halts immediately, even if the UI is unresponsive. The
          environment variable <code>JARVIS_AUTONOMY=off</code> does the same.
        </p>
      )}
    </section>
  );
}
