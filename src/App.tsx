import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
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
  rateMessage,
  reflectIfDue,
  sttCancel,
  sttDownloadModel,
  sttStart,
  sttStatus,
  sttStop,
  type SttReadiness,
  type Status,
  type Telemetry,
} from "./lib/ipc";
import { trustLabel, trustTone, trustWarning } from "./lib/confidence";
import { describeStatus } from "./lib/status";
import { platform, showSystemTelemetry } from "./lib/platform";
import {
  nextTheme,
  resolveInitialTheme,
  THEME_STORAGE_KEY,
  type Theme,
} from "./lib/theme";
import {
  chooseSttRoute,
  recognitionCtor,
  speak,
  stopSpeaking,
  STT_UNAVAILABLE_MESSAGE,
  sttAvailable,
  sttHint,
  ttsAvailable,
} from "./lib/voice";
import EventsView from "./views/EventsView";
import MemoryView from "./views/MemoryView";
import NotesView from "./views/NotesView";
import ReflectionsView from "./views/ReflectionsView";
import SettingsView from "./views/SettingsView";
import SkillsView from "./views/SkillsView";

interface ChatItem {
  key: string;
  role: "user" | "assistant" | "system";
  content: string;
  meta?: string;
  /// Set on assistant replies so the answer can be graded (calibration, §5.3).
  msgId?: number | null;
  /// Confidence v2: shown when calibration demoted a confident-looking answer.
  warning?: string | null;
  tone?: "high" | "mid" | "low" | null;
}

type Tab = "chat" | "skills" | "notes" | "memory" | "events" | "reflections" | "settings";

const TABS: { id: Tab; label: string; shortcut: string }[] = [
  { id: "chat", label: "chat", shortcut: "ctrl+1" },
  { id: "skills", label: "skills", shortcut: "ctrl+2" },
  { id: "notes", label: "notes", shortcut: "ctrl+3" },
  { id: "memory", label: "memory", shortcut: "ctrl+4" },
  { id: "events", label: "events", shortcut: "ctrl+5" },
  { id: "reflections", label: "reflections", shortcut: "ctrl+6" },
  { id: "settings", label: "settings", shortcut: "ctrl+7" },
];

const PALETTE_COMMANDS: PaletteCommand[] = [
  { id: "tab-chat", label: "Go to chat", hint: "ctrl+1" },
  { id: "tab-skills", label: "Go to skill library", hint: "ctrl+2" },
  { id: "tab-notes", label: "Go to notes", hint: "ctrl+3" },
  { id: "tab-memory", label: "Go to memory", hint: "ctrl+4" },
  { id: "tab-events", label: "Go to event log", hint: "ctrl+5" },
  { id: "tab-reflections", label: "Go to reflections", hint: "ctrl+6" },
  { id: "tab-settings", label: "Go to settings", hint: "ctrl+7" },
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
  const [lastConfidence, setLastConfidence] = useState<number | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [voiceOn, setVoiceOn] = useState(
    () => ttsAvailable && localStorage.getItem("jarvis.voice") === "on",
  );
  const [speaking, setSpeaking] = useState(false);
  const [listening, setListening] = useState(false);
  /// Local echo of grades given this session, so the buttons show their state.
  const [ratings, setRatings] = useState<Record<number, boolean>>({});
  const [stt, setStt] = useState<SttReadiness | null>(null);
  const [transcribing, setTranscribing] = useState(false);
  /// True only while the capture device is opening, so a double-click can't
  /// start a second take.
  const [starting, setStarting] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLInputElement>(null);
  const recognizerRef = useRef<{ stop: () => void } | null>(null);
  /// True while the in-flight take is the local (Rust-captured) one, so cleanup
  /// only cancels takes the backend actually owns.
  const localTakeRef = useRef(false);
  const tabBarRef = useRef<HTMLElement>(null);
  const underlineRef = useRef<HTMLSpanElement>(null);
  const underlineReady = useRef(false);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  }, [theme]);

  // Ask the backend how it can hear, so the mic button tells the truth.
  useEffect(() => {
    sttStatus().then(setStt).catch(() => setStt(null));
    // A take left running (window hidden to the tray, app closed, readiness
    // changing mid-session) would otherwise keep the capture thread alive and
    // the recorder slot occupied, so the next start fails with "already
    // recording". Cancelling is a no-op when idle, so this is unconditional.
    return () => {
      sttCancel().catch(() => {});
    };
  }, []);

  // If the local route stops being available while a *local* take is running —
  // the model is removed, or readiness resolves late — end the take rather than
  // leaving a thread capturing behind a button that now does something else.
  // Scoped to local takes: a web-recognizer take must not be cancelled here,
  // since its readiness is legitimately "not_compiled".
  useEffect(() => {
    if (listening && localTakeRef.current && stt?.state !== "ready") {
      localTakeRef.current = false;
      setListening(false);
      sttCancel().catch(() => {});
    }
  }, [listening, stt]);

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
    else if (id === "tab-reflections") setTab("reflections");
    else if (id === "tab-settings") setTab("settings");
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
      } else if (e.ctrlKey && ["1", "2", "3", "4", "5", "6", "7"].includes(e.key)) {
        e.preventDefault();
        setTab(TABS[Number(e.key) - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Shared-element indicator: glide the accent bar under the active tab. This
  // is a FLIP-style move done with transform only (translateX + scaleX off a
  // 1px base), so it's GPU-cheap and never touches layout. `animate: false`
  // snaps it (first paint, and on resize when tab widths change).
  const placeUnderline = useCallback((animate: boolean) => {
    const bar = tabBarRef.current;
    const underline = underlineRef.current;
    if (!bar || !underline) return;
    const active = bar.querySelector<HTMLButtonElement>('.tab[data-active="true"]');
    if (!active) return;
    if (!animate) underline.setAttribute("data-init", "true");
    underline.style.transform = `translateX(${active.offsetLeft}px) scaleX(${active.offsetWidth})`;
    if (!animate) {
      void underline.offsetWidth; // flush the no-transition placement
      underline.removeAttribute("data-init");
    }
  }, []);

  useLayoutEffect(() => {
    placeUnderline(underlineReady.current);
    underlineReady.current = true;
  }, [tab, placeUnderline]);

  useEffect(() => {
    const onResize = () => placeUnderline(false);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [placeUnderline]);

  const toggleVoice = useCallback(() => {
    setVoiceOn((on) => {
      const next = !on;
      localStorage.setItem("jarvis.voice", next ? "on" : "off");
      if (!next) {
        stopSpeaking();
        setSpeaking(false);
      }
      return next;
    });
  }, []);

  const say = useCallback((content: string) => {
    setItems((prev) => [...prev, { key: `s-${Date.now()}`, role: "system", content }]);
  }, []);

  // Voice v1: the local Whisper path. Audio is captured and transcribed in the
  // Rust core, which is the only way dictation can work inside WebView2 (it has
  // no SpeechRecognition) and the only way it stays on this machine.
  const toggleLocalListening = useCallback(async () => {
    // Guard both directions: opening the device can take hundreds of ms, and
    // without this a second click re-enters and the backend rejects it with
    // "already recording", surfacing a confusing error.
    if (transcribing || starting) return;
    if (listening) {
      setListening(false);
      localTakeRef.current = false;
      setTranscribing(true);
      try {
        const text = await sttStop();
        if (text) {
          setDraft((d) => (d ? `${d} ${text}` : text));
          composerRef.current?.focus();
        } else {
          say("I didn't catch anything — try again a little closer to the mic.");
        }
      } catch (e) {
        say(String(e));
      } finally {
        setTranscribing(false);
      }
      return;
    }
    stopSpeaking(); // barge-in
    setSpeaking(false);
    setStarting(true);
    try {
      await sttStart();
      localTakeRef.current = true;
      setListening(true);
    } catch (e) {
      say(String(e));
    } finally {
      setStarting(false);
    }
  }, [listening, transcribing, starting, say]);

  // Fetches the model once, then re-checks readiness so the button flips to
  // real push-to-talk without a restart.
  const downloadSttModel = useCallback(async () => {
    if (transcribing) return;
    setTranscribing(true);
    const mb = stt?.state === "needs_download" ? stt.approx_mb : 43;
    say(`Downloading the speech model (~${mb} MB, one time). This stays on your machine.`);
    try {
      await sttDownloadModel();
      const next = await sttStatus();
      setStt(next);
      say("Speech model ready — the mic button now runs entirely on your machine.");
    } catch (e) {
      say(String(e));
    } finally {
      setTranscribing(false);
    }
  }, [stt, transcribing, say]);

  // Push-to-talk: click to listen, click again (or silence) to stop.
  const toggleListening = useCallback(() => {
    const route = chooseSttRoute(stt, sttAvailable);
    if (route === "local") {
      void toggleLocalListening();
      return;
    }
    if (route === "download") {
      void downloadSttModel();
      return;
    }
    if (listening) {
      recognizerRef.current?.stop();
      return;
    }
    const Ctor = recognitionCtor();
    if (!Ctor) {
      setItems((prev) => [
        ...prev,
        { key: `s-${Date.now()}`, role: "system", content: STT_UNAVAILABLE_MESSAGE },
      ]);
      return;
    }
    stopSpeaking(); // barge-in: listening interrupts speech
    setSpeaking(false);
    const recognizer = new Ctor();
    recognizer.lang = "en-US";
    recognizer.interimResults = true;
    recognizer.maxAlternatives = 1;
    recognizer.onresult = (e) => {
      const transcript = Array.from({ length: e.results.length })
        .map((_, i) => e.results[i][0].transcript)
        .join(" ")
        .trim();
      setDraft(transcript);
    };
    recognizer.onend = () => {
      setListening(false);
      recognizerRef.current = null;
      composerRef.current?.focus();
    };
    recognizer.onerror = () => {
      setListening(false);
      recognizerRef.current = null;
    };
    recognizerRef.current = recognizer;
    setListening(true);
    recognizer.start();
  }, [listening, stt, toggleLocalListening, downloadSttModel]);

  const send = useCallback(async () => {
    const text = draft.trim();
    if (!text || busy) return;
    stopSpeaking(); // barge-in: a new message cuts Jarvis off
    setSpeaking(false);
    setDraft("");
    setItems((prev) => [
      ...prev,
      { key: `u-${Date.now()}`, role: "user", content: text },
    ]);
    setBusy(true);
    try {
      const reply = await chatSend(text);
      setLastConfidence(reply.confidence);
      if (voiceOn) {
        speak(reply.content, {
          onstart: () => setSpeaking(true),
          onend: () => setSpeaking(false),
        });
      }
      // Confidence v2: label by the calibrated value, and warn when the record
      // says a confident-looking answer is really a guess.
      const conf = trustLabel(reply.trust) ?? null;
      setItems((prev) => [
        ...prev,
        {
          key: `a-${Date.now()}`,
          role: "assistant",
          content: reply.content,
          meta: `${reply.provider} · ${reply.model}${reply.cached ? " · cached" : ""}${conf ? ` · ${conf}` : ""}`,
          msgId: reply.msg_id,
          warning: trustWarning(reply.trust),
          tone: trustTone(reply.trust),
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
  }, [draft, busy, voiceOn]);

  // Grading a reply is the one input calibration needs; the confidence it's
  // scored against is already in the event log. Re-rating corrects the record
  // rather than double-counting, so the buttons stay live after a click.
  const rate = useCallback((msgId: number, helpful: boolean) => {
    setRatings((prev) => ({ ...prev, [msgId]: helpful }));
    rateMessage(msgId, helpful).catch(() => {});
  }, []);

  const pill = describeStatus(status);
  const micRoute = chooseSttRoute(stt, sttAvailable);
  const coreState: CoreState = busy || transcribing || starting
    ? "thinking"
    : listening
      ? "listening"
      : speaking
        ? "speaking"
        : status?.ready
          ? "idle"
          : "offline";
  const systemTelemetry = showSystemTelemetry(platform);
  const messageCount = telemetry?.message_count ?? status?.message_count ?? 0;
  const factCount = telemetry?.fact_count ?? status?.fact_count ?? 0;

  return (
    <div className="hud">
      <header className="hud-header">
        <div className="brand">
          <span className="brand-name">H.O.T-JARVIS</span>
          <span className="brand-sub">local-first assistant · free forever</span>
        </div>
        <nav className="tab-bar" aria-label="views" ref={tabBarRef}>
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
          <span className="tab-underline" ref={underlineRef} aria-hidden="true" />
        </nav>
        {ttsAvailable && (
          <button
            type="button"
            className="theme-toggle"
            data-active={voiceOn}
            onClick={toggleVoice}
            aria-label={voiceOn ? "turn voice replies off" : "turn voice replies on"}
          >
            {voiceOn ? "voice on" : "voice off"}
          </button>
        )}
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
          {systemTelemetry && (
            <div className="readout">
              <span className="readout-label">cpu</span>
              <span className="readout-value">
                {telemetry ? `${Math.round(telemetry.cpu_percent)}%` : "—"}
              </span>
              <Sparkline values={cpuHistory} theme={theme} label="cpu history" />
            </div>
          )}
        </div>

        <ArcCore state={coreState} theme={theme} confidence={lastConfidence} />

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
              {!systemTelemetry
                ? "on-device · private" // iOS sandboxes system stats; say something true instead
                : telemetry
                  ? `${formatBytes(telemetry.mem_used)} / ${formatBytes(telemetry.mem_total)} · up ${formatDuration(telemetry.uptime_secs)}`
                  : "telemetry offline in browser preview"}
            </span>
          </div>
        </div>
      </section>

      <main className="view-area">
        {/* keyed on tab so each switch replays the enter animation (§6.2) */}
        <div className="view-swap" key={tab}>
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
                <div
                  key={item.key}
                  className="msg"
                  data-role={item.role}
                  data-tone={item.tone ?? undefined}
                >
                  {item.content}
                  {item.meta && <span className="msg-meta">{item.meta}</span>}
                  {item.warning && (
                    <span className="msg-warning" role="note">
                      {item.warning}
                    </span>
                  )}
                  {/* Grading a reply is what turns the self-rating into a
                      measurable track record (§5.3). */}
                  {item.role === "assistant" && typeof item.msgId === "number" && (
                    <span className="rate-row">
                      <button
                        type="button"
                        className="rate-btn"
                        data-picked={ratings[item.msgId] === true}
                        title="this answer was right and useful"
                        aria-label="mark answer helpful"
                        onClick={() => rate(item.msgId as number, true)}
                      >
                        ✓
                      </button>
                      <button
                        type="button"
                        className="rate-btn"
                        data-picked={ratings[item.msgId] === false}
                        title="this answer was wrong or unhelpful"
                        aria-label="mark answer not helpful"
                        onClick={() => rate(item.msgId as number, false)}
                      >
                        ✕
                      </button>
                    </span>
                  )}
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
                placeholder={listening ? "Listening…" : "Talk to Jarvis…"}
                aria-label="message"
                autoFocus
              />
              <button
                type="button"
                className="mic-btn"
                data-active={listening}
                data-busy={transcribing || starting}
                data-supported={micRoute !== "none"}
                title={sttHint(
                  micRoute,
                  stt?.state === "ready" ? stt.model : undefined,
                  stt?.state === "needs_download" ? stt.approx_mb : undefined,
                )}
                aria-label={
                  micRoute === "download"
                    ? "download the local speech model"
                    : listening
                      ? "stop listening and transcribe"
                      : "start voice input"
                }
                onClick={toggleListening}
              >
                {transcribing || starting ? "…" : listening ? "◉" : micRoute === "download" ? "⇩" : "🎙"}
              </button>
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
        {tab === "reflections" && <ReflectionsView />}
        {tab === "settings" && <SettingsView />}
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
        </div>
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
