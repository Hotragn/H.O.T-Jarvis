import { useCallback, useEffect, useState } from "react";
import { listNotes, readNote, saveNote } from "../lib/ipc";

// The notes tool's cockpit: list on the left, note body on the right.
// Backed by the Rust NotesTool — files live inside the app data dir only.
export default function NotesView() {
  const [notes, setNotes] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [body, setBody] = useState("");
  const [draftTitle, setDraftTitle] = useState("");
  const [draftBody, setDraftBody] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listNotes()
      .then(setNotes)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(refresh, [refresh]);

  const open = async (name: string) => {
    setSelected(name);
    setCreating(false);
    try {
      setBody(await readNote(name));
      setError(null);
    } catch (e) {
      setBody("");
      setError(String(e));
    }
  };

  const create = async () => {
    if (!draftTitle.trim() || !draftBody.trim()) return;
    try {
      const slug = await saveNote(draftTitle, draftBody);
      setDraftTitle("");
      setDraftBody("");
      setCreating(false);
      refresh();
      void open(slug);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="notes-view">
      <aside className="notes-list">
        <div className="panel-title-row">
          <span className="panel-title">notes</span>
          <button
            type="button"
            className="ghost-btn"
            onClick={() => {
              setCreating(true);
              setSelected(null);
            }}
          >
            + new
          </button>
        </div>
        {notes.length === 0 && !creating && (
          <p className="panel-hint">
            No notes yet. Jarvis stores them inside its own data folder.
          </p>
        )}
        <ul>
          {notes.map((name) => (
            <li key={name}>
              <button
                type="button"
                className="note-item"
                data-active={name === selected}
                onClick={() => void open(name)}
              >
                {name}
              </button>
            </li>
          ))}
        </ul>
      </aside>

      <section className="notes-body">
        {error && (
          <div className="msg" data-role="system">
            {error}
          </div>
        )}
        {creating ? (
          <div className="note-editor">
            <input
              className="chat-input"
              value={draftTitle}
              placeholder="Note title"
              aria-label="note title"
              onChange={(e) => setDraftTitle(e.target.value)}
            />
            <textarea
              className="chat-input note-textarea"
              value={draftBody}
              placeholder="Write the note…"
              aria-label="note content"
              onChange={(e) => setDraftBody(e.target.value)}
            />
            <div className="editor-actions">
              <button
                type="button"
                className="send-btn"
                disabled={!draftTitle.trim() || !draftBody.trim()}
                onClick={() => void create()}
              >
                Save note
              </button>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => setCreating(false)}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : selected ? (
          <article className="note-read">
            <h2 className="panel-title">{selected}</h2>
            <pre className="note-content">{body}</pre>
          </article>
        ) : (
          <div className="empty-state">
            <h1>Notes</h1>
            <p>Select a note on the left, or create a new one.</p>
          </div>
        )}
      </section>
    </div>
  );
}
