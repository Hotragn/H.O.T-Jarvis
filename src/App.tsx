import { useCallback, useEffect, useRef, useState } from "react";
import ArcCore, { type CoreState } from "./components/ArcCore";
import { chatSend, getHistory, getStatus, type Status } from "./lib/ipc";
import { describeStatus } from "./lib/status";
import {
  nextTheme,
  resolveInitialTheme,
  THEME_STORAGE_KEY,
  type Theme,
} from "./lib/theme";

interface ChatItem {
  key: string;
  role: "user" | "assistant" | "system";
  content: string;
  meta?: string;
}

function initialTheme(): Theme {
  return resolveInitialTheme(
    localStorage.getItem(THEME_STORAGE_KEY),
    window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
}

export default function App() {
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [status, setStatus] = useState<Status | null>(null);
  const [items, setItems] = useState<ChatItem[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  }, [theme]);

  useEffect(() => {
    getStatus().then(setStatus).catch(() => setStatus(null));
    getHistory()
      .then((history) =>
        setItems(
          history
            .filter((m) => m.role === "user" || m.role === "assistant")
            .map((m) => ({
              key: `db-${m.id}`,
              role: m.role as ChatItem["role"],
              content: m.content,
            })),
        ),
      )
      .catch(() => {});
  }, []);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [items, busy]);

  const send = useCallback(async () => {
    const text = draft.trim();
    if (!text || busy) return;
    setDraft("");
    setItems((prev) => [
      ...prev,
      { key: `u-${Date.now()}`, role: "user", content: text },
    ]);
    setBusy(true);
    try {
      const reply = await chatSend(text);
      setItems((prev) => [
        ...prev,
        {
          key: `a-${Date.now()}`,
          role: "assistant",
          content: reply.content,
          meta: `${reply.provider} · ${reply.model}`,
        },
      ]);
      getStatus().then(setStatus).catch(() => {});
    } catch (err) {
      setItems((prev) => [
        ...prev,
        { key: `s-${Date.now()}`, role: "system", content: String(err) },
      ]);
    } finally {
      setBusy(false);
    }
  }, [draft, busy]);

  const pill = describeStatus(status);
  const coreState: CoreState = busy
    ? "thinking"
    : status?.ready
      ? "idle"
      : "offline";

  return (
    <div className="hud">
      <header className="hud-header">
        <div className="brand">
          <span className="brand-name">H.O.T-JARVIS</span>
          <span className="brand-sub">local-first assistant · free forever</span>
        </div>
        <span className="version-tag">v0.1.0</span>
        <button
          type="button"
          className="theme-toggle"
          onClick={() => setTheme((t) => nextTheme(t))}
          aria-label={`switch to ${nextTheme(theme)} theme`}
        >
          {theme === "dark" ? "light" : "dark"}
        </button>
      </header>

      <section className="core-row">
        <div className="readout">
          <span className="readout-label">memory</span>
          <span className="readout-value">
            {status ? status.message_count : "—"}
          </span>
          <span className="readout-sub">
            messages held · {status ? status.fact_count : "—"} facts
          </span>
        </div>
        <ArcCore state={coreState} theme={theme} />
        <div className="readout" data-side="right">
          <span className="readout-label">model link</span>
          <span className="readout-value" data-tone={pill.tone}>
            {busy ? "thinking" : pill.tone === "ok" ? "online" : "standby"}
          </span>
          <span className="readout-sub">{pill.label}</span>
        </div>
      </section>

      <div className="chat-scroll" ref={scrollRef}>
        {status && !status.ready && status.onboarding && (
          <div className="msg" data-role="system">
            {status.onboarding}
          </div>
        )}
        {items.length === 0 && (!status || status.ready) && (
          <div className="empty-state">
            <h1>Ready when you are</h1>
            <p>Everything you say here is remembered locally — even after a restart.</p>
          </div>
        )}
        {items.map((item) => (
          <div key={item.key} className="msg" data-role={item.role}>
            {item.content}
            {item.meta && <span className="msg-meta">{item.meta}</span>}
          </div>
        ))}
        {busy && (
          <div className="msg thinking" data-role="assistant" aria-label="thinking">
            <i />
            <i />
            <i />
          </div>
        )}
      </div>

      <form
        className="composer"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <input
          className="chat-input"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Talk to Jarvis…"
          aria-label="message"
          autoFocus
        />
        <button className="send-btn" type="submit" disabled={busy || !draft.trim()}>
          Send
        </button>
      </form>
    </div>
  );
}
