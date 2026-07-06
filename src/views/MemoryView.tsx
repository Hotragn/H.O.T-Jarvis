import { useCallback, useEffect, useState } from "react";
import {
  exportMemory,
  getHistory,
  wipeMemory,
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
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(() => {
    getHistory(500)
      .then(setHistory)
      .catch((e) => setNotice(String(e)));
  }, []);

  useEffect(refresh, [refresh]);

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

  const doWipe = async () => {
    if (
      !window.confirm(
        "Erase all remembered messages and facts? This cannot be undone.",
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

      {notice && (
        <div className="msg" data-role="system">
          {notice}
        </div>
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
