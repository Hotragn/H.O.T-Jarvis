import { useEffect, useState } from "react";
import { eventDomain, summarizeEvent } from "../lib/events";
import { getEvents, type AppEvent } from "../lib/ipc";

// The action timeline: every event the assistant has logged, newest last.
// Read-only v0 of the replay & undo surface — inspection first, controls
// once the engine can actually replay and reverse.
export default function EventsView() {
  const [events, setEvents] = useState<AppEvent[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getEvents(300)
      .then(setEvents)
      .catch((e) => setError(String(e)));
    const timer = window.setInterval(() => {
      getEvents(300).then(setEvents).catch(() => {});
    }, 5000);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <div className="events-view">
      <div className="panel-title-row">
        <span className="panel-title">event log · {events.length} entries</span>
        <span className="panel-title">append-only · replay coming in m1</span>
      </div>

      {error && (
        <div className="msg" data-role="system">
          {error}
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
