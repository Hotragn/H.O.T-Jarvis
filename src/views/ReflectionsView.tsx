import { useCallback, useEffect, useMemo, useState } from "react";
import CalibrationPanel from "../components/CalibrationPanel";
import {
  calibrationReport,
  inTauri,
  listInsights,
  maintainInsights,
  reflectNow,
  type CalibrationReport,
  type Insight,
} from "../lib/ipc";

// The four lesson kinds the reflection pass can emit (see reflection.rs).
// Order here is the order the filter chips appear in.
const KINDS = ["skill", "provider", "user", "general"] as const;
type Kind = (typeof KINDS)[number];
type Filter = "all" | Kind;

const KIND_BLURB: Record<Kind, string> = {
  skill: "what worked or broke while writing and running skills",
  provider: "how the models and providers have been behaving",
  user: "what Jarvis has picked up about how you work",
  general: "everything else worth carrying forward",
};

// "events ..123" → "events up to #123" — make the provenance readable.
function readSource(source: string): string {
  const m = source.match(/events\s*\.\.\s*(\d+)/);
  return m ? `reflected on events up to #${m[1]}` : source;
}

// The reflection browser (§5.2, §6.x): a first-class window on the lessons
// Jarvis distils from its own event log. The memory view lists them in
// passing; here they get room — grouped by kind, with provenance and the
// controls to trigger a fresh pass.
export default function ReflectionsView() {
  const [insights, setInsights] = useState<Insight[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [reflecting, setReflecting] = useState(false);
  const [tidying, setTidying] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [calib, setCalib] = useState<CalibrationReport | null>(null);

  const refresh = useCallback(() => {
    listInsights(200)
      .then(setInsights)
      .catch((e) => setNotice(String(e)))
      .finally(() => setLoaded(true));
    calibrationReport()
      .then(setCalib)
      .catch(() => {});
  }, []);

  useEffect(refresh, [refresh]);

  const counts = useMemo(() => {
    const c: Record<Filter, number> = {
      all: insights.length,
      skill: 0,
      provider: 0,
      user: 0,
      general: 0,
    };
    for (const i of insights) {
      if ((KINDS as readonly string[]).includes(i.kind)) c[i.kind as Kind] += 1;
    }
    return c;
  }, [insights]);

  const shown = useMemo(
    () => (filter === "all" ? insights : insights.filter((i) => i.kind === filter)),
    [insights, filter],
  );

  const doReflect = async () => {
    if (reflecting) return;
    setReflecting(true);
    setNotice(null);
    try {
      const learned = await reflectNow();
      refresh();
      setNotice(
        learned.length > 0
          ? `Reflected: ${learned.length} new lesson${learned.length > 1 ? "s" : ""} learned.`
          : "Reflected: nothing new worth keeping yet.",
      );
    } catch (e) {
      setNotice(String(e));
    } finally {
      setReflecting(false);
    }
  };

  // Reflection v1: selective forgetting. Collapses duplicates into the lesson
  // they corroborate and drops what has faded, reporting exactly what went.
  const doTidy = async () => {
    if (tidying) return;
    setTidying(true);
    setNotice(null);
    try {
      const plan = await maintainInsights();
      refresh();
      if (!plan || (plan.forget.length === 0 && plan.merges.length === 0)) {
        setNotice("Nothing to tidy — every lesson is still earning its place.");
      } else {
        const merged = plan.merges.length;
        setNotice(
          `Forgot ${plan.forget.length} lesson${plan.forget.length === 1 ? "" : "s"}` +
            (merged > 0 ? ` (${merged} merged into the lesson they confirm)` : "") +
            `, kept ${plan.kept}. Check the event log for the reason behind each one.`,
        );
      }
    } catch (e) {
      setNotice(String(e));
    } finally {
      setTidying(false);
    }
  };

  return (
    <div className="reflections-view">
      <div className="panel-title-row">
        <span className="panel-title">
          reasoning-memory · {insights.length} lesson{insights.length === 1 ? "" : "s"}
        </span>
        <span className="editor-actions">
          <button
            type="button"
            className="ghost-btn"
            disabled={reflecting || !inTauri}
            title={inTauri ? "run a reflection pass now" : "launch the app to reflect"}
            onClick={() => void doReflect()}
          >
            {reflecting ? "Reflecting…" : "Reflect now"}
          </button>
          <button
            type="button"
            className="ghost-btn"
            disabled={tidying || !inTauri || insights.length === 0}
            title="merge duplicate lessons and forget the ones that have faded"
            onClick={() => void doTidy()}
          >
            {tidying ? "Tidying…" : "Tidy up"}
          </button>
        </span>
      </div>

      <p className="panel-hint">
        After enough activity Jarvis re-reads its own event log and keeps short
        lessons about what worked and what failed. They ride along in future
        prompts, so the assistant gets a little sharper the more you use it.
      </p>

      {notice && (
        <div className="msg" data-role="system">
          {notice}
        </div>
      )}

      <CalibrationPanel report={calib} />

      {insights.length > 0 && (
        <div className="reflect-filters" role="tablist" aria-label="filter lessons by kind">
          {(["all", ...KINDS] as Filter[]).map((f) => (
            <button
              key={f}
              type="button"
              role="tab"
              aria-selected={filter === f}
              className="reflect-chip"
              data-kind={f}
              data-active={filter === f}
              disabled={f !== "all" && counts[f] === 0}
              onClick={() => setFilter(f)}
            >
              {f} <span className="reflect-chip-count">{counts[f]}</span>
            </button>
          ))}
        </div>
      )}

      {shown.length === 0 ? (
        <div className="empty-state">
          <h1>{loaded ? "No lessons yet" : "Loading…"}</h1>
          <p>
            {!inTauri
              ? "This is the browser preview. Launch the app with npm run tauri dev to see Jarvis reflect on real activity."
              : filter === "all"
                ? "Keep talking to Jarvis. Once there is enough in the event log, a reflection pass distils the first lessons here."
                : `No ${filter} lessons so far. Try another kind, or run a reflection pass.`}
          </p>
        </div>
      ) : (
        <ul className="reflect-list">
          {shown.map((i) => {
            const kind = (KINDS as readonly string[]).includes(i.kind)
              ? (i.kind as Kind)
              : "general";
            return (
              <li key={i.id} className="reflect-card" data-kind={kind}>
                <div className="reflect-card-head">
                  <span className="reflect-badge" data-kind={kind}>
                    {kind}
                  </span>
                  <time className="reflect-when">
                    {i.created_at
                      ? new Date(i.created_at * 1000).toLocaleDateString(undefined, {
                          month: "short",
                          day: "numeric",
                        })
                      : "just now"}
                  </time>
                </div>
                <p className="reflect-text">{i.content}</p>
                <p className="reflect-source" title={KIND_BLURB[kind]}>
                  {readSource(i.source)}
                  {i.corroborations > 0 && (
                    <span className="reflect-evidence" title="times a later reflection independently re-derived this lesson">
                      {" · confirmed "}
                      {i.corroborations}
                      {i.corroborations === 1 ? " time" : " times"}
                    </span>
                  )}
                </p>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
