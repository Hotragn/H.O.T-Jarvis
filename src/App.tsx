import { useCallback, useEffect, useRef, useState } from "react";
import ArcCore, { type CoreState } from "./components/ArcCore";
import CommandPalette from "./components/CommandPalette";
import Sparkline from "./components/Sparkline";
import type { PaletteCommand } from "./lib/commands";
import { formatBytes, formatClock, formatDuration } from "./lib/format";
import {
  chatSend,
  getHistory,
  getStatus,
  getTelemetry,
  reflectIfDue,
  type Status,
  type Telemetry,
} from "./lib/ipc";
import { describeStatus } from "./lib/status";
import {
  nextTheme,
  resolveInitialTheme,
  THEME_STORAGE_KEY,
  type Theme,
} from "./lib/theme";
import EventsView from "./views/EventsView";
import MemoryView from "./views/MemoryView";
import NotesView from "./views/NotesView";
import SkillsView from "./views/SkillsView";

interface ChatItem {
  key: string;
  role: "user" | "assistant" | "system";
  content: string;
  meta?: string;
}

type Tab = "chat" | "skills" | "notes" | "memory" | "events";

const TABS: { id: Tab; label: string; shortcut: string }[] = [
  { id: "chat", label: "chat", shortcut: "ctrl+1" },
  { id: "skills", label: "skills", shortcut: "ctrl+2" },
  { id: "notes", label: "notes", shortcut: "ctrl+3" },
  { id: "memory", label: "memory", shortcut: "ctrl+4" },
  { id: "events", label: "events", shortcut: "ctrl+5" },
];

const PALETTE_COMMANDS: PaletteCommand[] = [
  { id: "tab-chat", label: "Go to chat", hint: "ctrl+1" },
  { id: "tab-skills", label: "Go to skill library", hint: "ctrl+2" },
  { id: "tab-notes", label: "Go to notes", hint: "ctrl+3" },
  { id: "tab-memory", label: "Go to memory", hint: "ctrl+4" },
  { id: "tab-events", label: "Go to event log", hint: "ctrl+5" },
  { id: "focus-composer", label: "Talk to Jarvis", hint: "chat" },
  { id: "theme-toggle", label: "Toggle theme" },
];

function initialTheme(): Theme {
  return resolveInitialTheme(
    localStorage.getItem(THEME_STORAGE_KEY),
    window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
}

export default function App() {
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [tab, setTab] = useState<Tab>("chat");
  const [status, setStatus] = useState<Status | null>(null);
  const [telemetry, setTelemetry] = useState<Telemetry | null>(null);
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);
  const [clock, setClock] = useState(() => formatClock(new Date()));
  const [items, setItems] = useState<ChatItem[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLInputElement>(null);

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

  // Live vitals: telemetry every 2s, wall clock every second.
  useEffect(() => {
    const poll = () => {
      getTelemetry()
        .then((t) => {
          if (!t) return;
          setTelemetry(t);
          setCpuHistory((h) => [...h.slice(-39), t.cpu_percent]);
        })
        .catch(() => {});
    };
    poll();
    const telemetryTimer = window.setInterval(poll, 2000);
    const clockTimer = window.setInterval(
      () => setClock(formatClock(new Date())),
      1000,
    );
    return () => {
      window.clearInterval(telemetryTimer);
      window.clearInterval(clockTimer);
    };
  }, []);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [items, busy]);

  const runCommand = useCallback((id: string) => {
    setPaletteOpen(false);
    if (id === "tab-chat") setTab("chat");
    else if (id === "tab-skills") setTab("skills");
    else if (id === "tab-notes") setTab("notes");
    else if (id === "tab-memory") setTab("memory");
    else if (id === "tab-events") setTab("events");
    else if (id === "theme-toggle") setTheme((t) => nextTheme(t));
    else if (id === "focus-composer") {
      setTab("chat");
      window.setTimeout(() => composerRef.current?.focus(), 60);
    }
  }, []);

  // Global keys: Ctrl+K palette, Ctrl+1/2/3 tabs.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((open) => !open);
      } else if (e.ctrlKey && ["1", "2", "3", "4", "5"].includes(e.key)) {
        e.preventDefault();
        setTab(TABS[Number(e.key) - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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
          meta: `${reply.provider} · ${reply.model}${reply.cached ? " · cached" : ""}`,
        },
      ]);
      getStatus().then(setStatus).catch(() => {});
      // Periodic reflection: fires for real only when enough conversation
      // has accumulated since the last pass.
      reflectIfDue().catch(() => {});
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
  const messageCount = telemetry?.message_count ?? status?.message_count ?? 0;
  const factCount = telemetry?.fact_count ?? status?.fact_count ?? 0;

  return (
    <div className="hud">
      <header className="hud-header">
        <div className="brand">
          <span className="brand-name">H.O.T-JARVIS</span>
          <span className="brand-sub">local-first assistant · free forever</span>
        </div>
        <nav className="tab-bar" aria-label="views">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className="tab"
              data-active={tab === t.id}
              title={t.shortcut}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
        <button
          type="button"
          className="theme-toggle"
          onClick={() => setPaletteOpen(true)}
          aria-label="open command palette"
        >
          ctrl+k
        </button>
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
        <i className="trace" data-pos="l1" aria-hidden="true" />
        <i className="trace" data-pos="l2" aria-hidden="true" />
        <i className="trace" data-pos="r1" aria-hidden="true" />
        <i className="trace" data-pos="r2" aria-hidden="true" />

        <div className="readout-stack">
          <div className="readout">
            <span className="readout-label">memory</span>
            <span className="readout-value">{status ? messageCount : "—"}</span>
            <span className="readout-sub">
              messages held · {status ? factCount : "—"} facts ·{" "}
              {telemetry ? telemetry.note_count : "—"} notes
            </span>
          </div>
          <div className="readout">
            <span className="readout-label">cpu</span>
            <span className="readout-value">
              {telemetry ? `${Math.round(telemetry.cpu_percent)}%` : "—"}
            </span>
            <Sparkline values={cpuHistory} theme={theme} label="cpu history" />
          </div>
        </div>

        <ArcCore state={coreState} theme={theme} />

        <div className="readout-stack">
          <div className="readout" data-side="right">
            <span className="readout-label">model link</span>
            <span className="readout-value" data-tone={pill.tone}>
              {busy ? "thinking" : pill.tone === "ok" ? "online" : "standby"}
            </span>
            <span className="readout-sub">{pill.label}</span>
          </div>
          <div className="readout" data-side="right">
            <span className="readout-label">system</span>
            <span className="readout-value">{clock}</span>
            <span className="readout-sub">
              {telemetry
                ? `${formatBytes(telemetry.mem_used)} / ${formatBytes(telemetry.mem_total)} · up ${formatDuration(telemetry.uptime_secs)}`
                : "telemetry offline in browser preview"}
            </span>
          </div>
        </div>
      </section>

      <main className="view-area">
        {tab === "chat" && (
          <>
            <div className="chat-scroll" ref={scrollRef}>
              {status && !status.ready && status.onboarding && (
                <div className="msg" data-role="system">
                  {status.onboarding}
                </div>
              )}
              {items.length === 0 && (!status || status.ready) && (
                <div className="empty-state">
                  <h1>Ready when you are</h1>
                  <p>
                    Everything you say here is remembered locally — even after a
                    restart.
                  </p>
                </div>
              )}
              {items.map((item) => (
                <div key={item.key} className="msg" data-role={item.role}>
                  {item.content}
                  {item.meta && <span className="msg-meta">{item.meta}</span>}
                </div>
              ))}
              {busy && (
                <div
                  className="msg thinking"
                  data-role="assistant"
                  aria-label="thinking"
                >
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
                ref={composerRef}
                className="chat-input"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                placeholder="Talk to Jarvis…"
                aria-label="message"
                autoFocus
              />
              <button
                className="send-btn"
                type="submit"
                disabled={busy || !draft.trim()}
              >
                Send
              </button>
            </form>
          </>
        )}
        {tab === "skills" && <SkillsView />}
        {tab === "notes" && <NotesView />}
        {tab === "events" && <EventsView />}
        {tab === "memory" && (
          <MemoryView
            messageCount={messageCount}
            factCount={factCount}
            onWiped={() => {
              setItems([]);
              getStatus().then(setStatus).catch(() => {});
            }}
          />
        )}
      </main>

      {paletteOpen && (
        <CommandPalette
          commands={PALETTE_COMMANDS}
          onRun={runCommand}
          onClose={() => setPaletteOpen(false)}
        />
      )}
    </div>
  );
}
