import { useCallback, useEffect, useState } from "react";
import { eventDomain, summarizeEvent } from "../lib/events";
import {
  getEvents,
  replayAudit,
  undoEvent,
  type AppEvent,
  type ReplayReport,
} from "../lib/ipc";
import { isReversible } from "../lib/undo";

// The action timeline (§5.4): inspect everything, undo what's reversible,
// and audit that the log deterministically reproduces the live memory.
// Undos append to the timeline — history is never rewritten.
export default function EventsView() {
  const [events, setEvents] = useState<AppEvent[]>([]);
  const [report, setReport] = useState<ReplayReport | null>(null);
  const [auditing, setAuditing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(() => {
    getEvents(300)
      .then(setEvents)
      .catch((e) => setNotice(String(e)));
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const doUndo = async (id: number) => {
    try {
      setNotice(await undoEvent(id));
      refresh();
    } catch (e) {
      setNotice(String(e));
    }
  };

  const doAudit = async () => {
    if (auditing) return;
    setAuditing(true);
    setReport(null);
    try {
      setReport(await replayAudit());
      refresh();
    } catch (e) {
      setNotice(String(e));
    } finally {
      setAuditing(false);
    }
  };

  return (
    <div className="events-view">
      <div className="panel-title-row">
        <span className="panel-title">event log · {events.length} entries</span>
        <button
          type="button"
          className="ghost-btn"
          disabled={auditing}
          onClick={() => void doAudit()}
        >
          {auditing ? "Auditing…" : "Replay audit"}
        </button>
      </div>

      {notice && (
        <div className="msg" data-role="system">
          {notice}
        </div>
      )}

      {report && (
        <div
          className="msg"
          data-role="system"
          data-audit={report.deterministic ? "ok" : "drift"}
        >
          {report.deterministic
            ? `Replay audit: deterministic — the log reproduces all ${report.matched} messages in memory exactly.`
            : `Replay audit: drift detected — ${report.matched} matched, ${report.missing_in_db.length} in the log but missing from memory, ${report.extra_in_db.length} in memory but not in the log.`}
        </div>
      )}

      {events.length === 0 ? (
        <div className="empty-state">
          <h1>No events yet</h1>
          <p>Every action Jarvis takes is recorded here, permanently and locally.</p>
        </div>
      ) : (
        <ul className="memory-list">
          {events.map((e) => (
            <li key={e.id} className="event-row" data-domain={eventDomain(e.kind)}>
              <span className="event-id">#{e.id}</span>
              <span className="event-kind">{e.kind}</span>
              <span className="memory-text">{summarizeEvent(e)}</span>
              {isReversible(e.kind) ? (
                <button
                  type="button"
                  className="ghost-btn undo-btn"
                  title="reverse this action"
                  onClick={() => void doUndo(e.id)}
                >
                  undo
                </button>
              ) : (
                <span className="undo-spacer" aria-hidden="true" />
              )}
              <time className="memory-time">
                {new Date(e.ts * 1000).toLocaleTimeString()}
              </time>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
