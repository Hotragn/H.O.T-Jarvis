import { useEffect, useState } from "react";
import AutonomyPanel from "../components/AutonomyPanel";
import {
  getProviderSettings,
  inTauri,
  setProviderSettings,
  type ProviderSettings,
} from "../lib/ipc";

const EMPTY: ProviderSettings = {
  ollama_base_url: "",
  ollama_model: "",
  groq_api_key: "",
  groq_model: "",
  openrouter_api_key: "",
  openrouter_model: "",
};

// Providers and models, configurable at runtime — no .env edit, no restart.
// This view is also what makes companion mode real: on a phone (or a laptop
// away from home) point the Ollama URL at a machine that runs it, and that
// machine becomes the brain while everything else stays local to this device.
export default function SettingsView() {
  const [form, setForm] = useState<ProviderSettings>(EMPTY);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    getProviderSettings()
      .then((s) => {
        if (s) setForm(s);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  const set =
    (key: keyof ProviderSettings) =>
    (e: React.ChangeEvent<HTMLInputElement>) =>
      setForm((f) => ({ ...f, [key]: e.target.value }));

  const save = async () => {
    if (saving) return;
    setSaving(true);
    setNotice(null);
    try {
      const reachable = await setProviderSettings(form);
      setNotice(
        reachable
          ? "Saved and applied. The local model endpoint is reachable."
          : "Saved and applied. The local endpoint isn't answering — check the URL, or rely on a cloud key if you set one.",
      );
    } catch (e) {
      setNotice(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="settings-view">
      <AutonomyPanel />

      <div className="panel-title-row">
        <span className="panel-title">providers &amp; models</span>
        <button
          type="button"
          className="ghost-btn"
          disabled={saving || !inTauri}
          title={inTauri ? "save and apply immediately" : "launch the app to change settings"}
          onClick={() => void save()}
        >
          {saving ? "Applying…" : "Save & apply"}
        </button>
      </div>

      <p className="panel-hint">
        Changes apply immediately — no restart, no .env file. Keys are stored
        only on this device and never leave it except to call the provider you
        gave them for.
      </p>

      {notice && (
        <div className="msg" data-role="system">
          {notice}
        </div>
      )}

      {!loaded ? (
        <p className="panel-hint">Loading…</p>
      ) : (
        <div className="settings-grid">
          <fieldset className="settings-group">
            <legend>Local — Ollama (private, unlimited, free)</legend>
            <label className="settings-field">
              <span>Server URL</span>
              <input
                className="chat-input"
                value={form.ollama_base_url}
                placeholder="http://localhost:11434"
                onChange={set("ollama_base_url")}
              />
            </label>
            <label className="settings-field">
              <span>Model</span>
              <input
                className="chat-input"
                value={form.ollama_model}
                placeholder="llama3.2"
                onChange={set("ollama_model")}
              />
            </label>
            <p className="settings-note">
              Companion mode: on another device, set the URL to this machine's
              address (e.g. http://192.168.1.20:11434) and it does the thinking
              — private, over your own network. Start Ollama with
              OLLAMA_HOST=0.0.0.0 to allow that.
            </p>
          </fieldset>

          <fieldset className="settings-group">
            <legend>Cloud fallback — Groq (free tier)</legend>
            <label className="settings-field">
              <span>API key</span>
              <input
                className="chat-input"
                type="password"
                value={form.groq_api_key}
                placeholder="gsk_…"
                autoComplete="off"
                onChange={set("groq_api_key")}
              />
            </label>
            <label className="settings-field">
              <span>Model</span>
              <input
                className="chat-input"
                value={form.groq_model}
                placeholder="llama-3.3-70b-versatile"
                onChange={set("groq_model")}
              />
            </label>
          </fieldset>

          <fieldset className="settings-group">
            <legend>Cloud fallback — OpenRouter (:free models)</legend>
            <label className="settings-field">
              <span>API key</span>
              <input
                className="chat-input"
                type="password"
                value={form.openrouter_api_key}
                placeholder="sk-or-…"
                autoComplete="off"
                onChange={set("openrouter_api_key")}
              />
            </label>
            <label className="settings-field">
              <span>Model</span>
              <input
                className="chat-input"
                value={form.openrouter_model}
                placeholder="meta-llama/llama-3.3-70b-instruct:free"
                onChange={set("openrouter_model")}
              />
            </label>
            <p className="settings-note">
              Cloud calls send your prompt to that provider. Local stays the
              default whenever it's reachable.
            </p>
          </fieldset>
        </div>
      )}
    </div>
  );
}
