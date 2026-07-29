import { useCallback, useEffect, useState } from "react";
import {
  exportMemory,
  getHistory,
  indexMemory,
  inTauri,
  listInsights,
  reflectNow,
  searchMemory,
  wipeMemory,
  type Insight,
  type SearchHit,
  type StoredMessage,
} from "../lib/ipc";

interface Props {
  messageCount: number;
  factCount: number;
  onWiped: () => void;
}

// The memory browser: what Jarvis remembers, and the owner's controls over
// it — export everything as JSON, or wipe it. Their data, their call.
export default function MemoryView({ messageCount, factCount, onWiped }: Props) {
  const [history, setHistory] = useState<StoredMessage[]>([]);
  const [insights, setInsights] = useState<Insight[]>([]);
  const [reflecting, setReflecting] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [indexing, setIndexing] = useState(false);

  const refresh = useCallback(() => {
    getHistory(500)
      .then(setHistory)
      .catch((e) => setNotice(String(e)));
    listInsights(50)
      .then(setInsights)
      .catch(() => {});
  }, []);

  useEffect(refresh, [refresh]);

  // Meaning-based search over the whole history — local embeddings, so this
  // works offline and nothing leaves the machine.
  const doSearch = async () => {
    const q = query.trim();
    if (!q || searching) return;
    setSearching(true);
    setNotice(null);
    try {
      setHits(await searchMemory(q, 10));
    } catch (e) {
      // Most common cause: the embedding model isn't pulled yet.
      setNotice(String(e));
      setHits(null);
    } finally {
      setSearching(false);
    }
  };

  // Backfills embeddings for history from before semantic memory existed.
  const doIndex = async () => {
    if (indexing) return;
    setIndexing(true);
    setNotice(null);
    try {
      const [indexed, remaining] = await indexMemory();
      setNotice(
        remaining > 0
          ? `Indexed ${indexed} messages; ${remaining} to go — click again to continue.`
          : indexed > 0
            ? `Indexed ${indexed} messages. Everything is searchable.`
            : "Everything is already indexed.",
      );
    } catch (e) {
      setNotice(String(e));
    } finally {
      setIndexing(false);
    }
  };

  const doExport = async () => {
    try {
      const dump = await exportMemory();
      const blob = new Blob([JSON.stringify(dump, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `jarvis-memory-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      setNotice("Memory exported as JSON.");
    } catch (e) {
      setNotice(String(e));
    }
  };

  const doReflect = async () => {
    if (reflecting) return;
    setReflecting(true);
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

  const doWipe = async () => {
    if (
      !window.confirm(
        "Erase all remembered messages, facts, and the event log? Notes are kept. This cannot be undone.",
      )
    )
      return;
    try {
      await wipeMemory();
      refresh();
      onWiped();
      setNotice("Memory wiped.");
    } catch (e) {
      setNotice(String(e));
    }
  };

  return (
    <div className="memory-view">
      <div className="panel-title-row">
        <span className="panel-title">
          memory · {messageCount} messages · {factCount} facts
        </span>
        <span className="editor-actions">
          <button type="button" className="ghost-btn" onClick={() => void doExport()}>
            Export JSON
          </button>
          <button type="button" className="ghost-btn danger" onClick={() => void doWipe()}>
            Wipe…
          </button>
        </span>
      </div>

      <form
        className="memory-search"
        onSubmit={(e) => {
          e.preventDefault();
          void doSearch();
        }}
      >
        <input
          className="chat-input"
          value={query}
          placeholder='Search by meaning — e.g. "that note about the telescope"'
          aria-label="semantic memory search"
          onChange={(e) => {
            setQuery(e.target.value);
            if (!e.target.value.trim()) setHits(null);
          }}
        />
        <button type="submit" className="ghost-btn" disabled={searching || !inTauri || !query.trim()}>
          {searching ? "Searching…" : "Search"}
        </button>
        <button
          type="button"
          className="ghost-btn"
          disabled={indexing || !inTauri}
          title="embed older history so it becomes searchable"
          onClick={() => void doIndex()}
        >
          {indexing ? "Indexing…" : "Build index"}
        </button>
      </form>

      {notice && (
        <div className="msg" data-role="system">
          {notice}
        </div>
      )}

      {hits !== null && (
        <>
          <div className="panel-title-row">
            <span className="panel-title">matches · {hits.length}</span>
            <button type="button" className="ghost-btn" onClick={() => { setHits(null); setQuery(""); }}>
              Clear
            </button>
          </div>
          {hits.length === 0 ? (
            <p className="panel-hint">
              Nothing similar found. Recall works by meaning, so try describing
              the moment rather than quoting it.
            </p>
          ) : (
            <ul className="memory-list">
              {hits.map((h) => (
                <li key={h.id} className="memory-row" data-role={h.role}>
                  <span className="memory-role">{Math.round(h.score * 100)}%</span>
                  <span className="memory-text">{h.content}</span>
                  <time className="memory-time">
                    {new Date(h.created_at * 1000).toLocaleString()}
                  </time>
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      <div className="panel-title-row">
        <span className="panel-title">
          lessons learned · {insights.length}
        </span>
        <button
          type="button"
          className="ghost-btn"
          disabled={reflecting}
          onClick={() => void doReflect()}
        >
          {reflecting ? "Reflecting…" : "Reflect now"}
        </button>
      </div>
      {insights.length === 0 ? (
        <p className="panel-hint">
          After enough activity, Jarvis re-reads its own event log and keeps
          short lessons about what worked and what failed. They ride along in
          future prompts.
        </p>
      ) : (
        <ul className="memory-list insights-list">
          {insights.map((i) => (
            <li key={i.id} className="memory-row" data-role="assistant">
              <span className="memory-role">{i.kind}</span>
              <span className="memory-text">{i.content}</span>
              <time className="memory-time">
                {new Date(i.created_at * 1000).toLocaleDateString()}
              </time>
            </li>
          ))}
        </ul>
      )}

      {history.length === 0 ? (
        <div className="empty-state">
          <h1>Nothing remembered yet</h1>
          <p>Conversations are stored locally and appear here.</p>
        </div>
      ) : (
        <ul className="memory-list">
          {history.map((m) => (
            <li key={m.id} className="memory-row" data-role={m.role}>
              <span className="memory-role">{m.role}</span>
              <span className="memory-text">{m.content}</span>
              <time className="memory-time">
                {new Date(m.created_at * 1000).toLocaleString()}
              </time>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
