import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  inTauri,
  replayAuditState,
  replayStateAt,
  replayTimeline,
  type ReplayState,
  type ReplayStep,
  type StateReport,
} from "../lib/ipc";

// Replay v2 (§5.4): the step-through session player.
//
// v1 could prove the log reproduced memory; it couldn't show you. This walks the
// event log one frame at a time and reconstructs the whole world at that point —
// messages, notes, skills, lessons — so "nothing it does is permanent" becomes
// something you can watch rather than take on trust.
//
// Reconstruction happens in the Rust core (pure, tested); this is the scrubber.
export default function SessionPlayer() {
  const [steps, setSteps] = useState<ReplayStep[]>([]);
  const [at, setAt] = useState(0);
  const [snapshot, setSnapshot] = useState<ReplayState | null>(null);
  const [report, setReport] = useState<StateReport | null>(null);
  const [playing, setPlaying] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [onlyChanges, setOnlyChanges] = useState(true);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    replayTimeline()
      .then((t) => {
        setSteps(t);
        setAt(t.length);
      })
      .catch((e) => setNotice(String(e)));
  }, []);

  // Keep the reconstructed snapshot in step with the scrubber.
  useEffect(() => {
    let cancelled = false;
    replayStateAt(at)
      .then((s) => {
        if (!cancelled) setSnapshot(s);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [at]);

  // Frames worth landing on: most events change nothing, and stepping through
  // startups and telemetry would bury the moments that matter.
  const stops = useMemo(
    () => (onlyChanges ? steps.filter((s) => s.changed) : steps),
    [steps, onlyChanges],
  );

  const advance = useCallback(() => {
    setAt((current) => {
      const next = stops.find((s) => s.step > current);
      return next ? next.step : current;
    });
  }, [stops]);

  // Playback: walk the interesting frames, stopping at the end.
  useEffect(() => {
    if (!playing) return;
    const last = stops.length > 0 ? stops[stops.length - 1].step : 0;
    if (at >= last) {
      setPlaying(false);
      return;
    }
    const timer = window.setTimeout(advance, 700);
    return () => window.clearTimeout(timer);
  }, [playing, at, stops, advance]);

  const back = () =>
    setAt((current) => {
      const prior = [...stops].reverse().find((s) => s.step < current);
      return prior ? prior.step : 0;
    });

  const runAudit = async () => {
    setNotice(null);
    try {
      setReport(await replayAuditState());
    } catch (e) {
      setNotice(String(e));
    }
  };

  const current = steps.find((s) => s.step === at) ?? null;

  return (
    <section className="player" aria-label="session player">
      <div className="panel-title-row">
        <span className="panel-title">
          session player · step {at} / {steps.length}
        </span>
        <span className="editor-actions">
          <button
            type="button"
            className="ghost-btn"
            disabled={!inTauri || steps.length === 0}
            onClick={() => setAt(0)}
            title="jump to the beginning"
          >
            ⏮
          </button>
          <button
            type="button"
            className="ghost-btn"
            disabled={!inTauri || at === 0}
            onClick={back}
            aria-label="previous change"
          >
            ◀
          </button>
          <button
            type="button"
            className="ghost-btn"
            disabled={!inTauri || steps.length === 0}
            onClick={() => setPlaying((p) => !p)}
          >
            {playing ? "Pause" : "Play"}
          </button>
          <button
            type="button"
            className="ghost-btn"
            disabled={!inTauri || at >= steps.length}
            onClick={advance}
            aria-label="next change"
          >
            ▶
          </button>
          <button
            type="button"
            className="ghost-btn"
            data-active={onlyChanges}
            onClick={() => setOnlyChanges((v) => !v)}
            title="skip events that changed nothing"
          >
            {onlyChanges ? "changes only" : "every event"}
          </button>
        </span>
      </div>

      {!inTauri ? (
        <p className="panel-hint">
          The player replays the real event log — launch the app to use it.
        </p>
      ) : steps.length === 0 ? (
        <p className="panel-hint">
          Nothing recorded yet. Talk to Jarvis and the session becomes
          replayable, step by step.
        </p>
      ) : (
        <>
          <input
            className="player-scrub"
            type="range"
            min={0}
            max={steps.length}
            value={at}
            aria-label="scrub through the session"
            onChange={(e) => {
              setPlaying(false);
              setAt(Number(e.target.value));
            }}
          />

          <p className="player-now">
            {current ? current.summary : "before anything happened"}
          </p>

          {/* The reconstructed world at this exact point. */}
          <div className="player-state">
            <span>
              <b>{snapshot?.messages.length ?? 0}</b> messages
            </span>
            <span>
              <b>{Object.keys(snapshot?.notes ?? {}).length}</b> notes
            </span>
            <span>
              <b>{Object.keys(snapshot?.skills ?? {}).length}</b> skills
            </span>
            <span>
              <b>{snapshot?.insights.length ?? 0}</b> lessons
            </span>
          </div>

          <ul className="player-frames" ref={listRef}>
            {stops.slice(-40).map((s) => (
              <li key={s.step}>
                <button
                  type="button"
                  className="player-frame"
                  data-active={s.step === at}
                  onClick={() => {
                    setPlaying(false);
                    setAt(s.step);
                  }}
                >
                  <span className="player-frame-step">{s.step}</span>
                  <span className="player-frame-summary">{s.summary}</span>
                </button>
              </li>
            ))}
          </ul>

          <div className="panel-title-row">
            <span className="panel-title">determinism audit</span>
            <button type="button" className="ghost-btn" onClick={() => void runAudit()}>
              Check log vs reality
            </button>
          </div>

          {notice && (
            <div className="msg" data-role="system">
              {notice}
            </div>
          )}

          {report && (
            <div className="msg" data-audit={report.deterministic ? "ok" : undefined}>
              {report.summary}
            </div>
          )}
        </>
      )}
    </section>
  );
}
