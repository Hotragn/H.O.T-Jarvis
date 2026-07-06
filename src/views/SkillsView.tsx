import { useCallback, useEffect, useState } from "react";
import {
  listSkills,
  runSkill,
  saveSkill,
  testSkill,
  type SkillManifest,
} from "../lib/ipc";
import { testBadge } from "../lib/skills";

const CODE_TEMPLATE = `fn run(input) {
  // Rhai script: transform the input and return a value.
  "You said: " + input
}`;

const TEST_TEMPLATE = `fn test() {
  // Must return true for the skill to be usable.
  run("hi") == "You said: hi"
}`;

// The skill library (§5.1): every skill is code plus a bundled test, and its
// test status is always visible. Failing skills are flagged, not runnable.
export default function SkillsView() {
  const [skills, setSkills] = useState<SkillManifest[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftDesc, setDraftDesc] = useState("");
  const [draftCode, setDraftCode] = useState(CODE_TEMPLATE);
  const [draftTest, setDraftTest] = useState(TEST_TEMPLATE);
  const [runInput, setRunInput] = useState("");
  const [runOutput, setRunOutput] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listSkills()
      .then(setSkills)
      .catch((e) => setNotice(String(e)));
  }, []);

  useEffect(refresh, [refresh]);

  const current = skills.find((s) => s.name === selected) ?? null;

  const create = async () => {
    if (!draftName.trim()) return;
    try {
      const saved = await saveSkill(draftName, draftDesc, draftCode, draftTest);
      setCreating(false);
      setDraftName("");
      setDraftDesc("");
      setDraftCode(CODE_TEMPLATE);
      setDraftTest(TEST_TEMPLATE);
      refresh();
      setSelected(saved.name);
      setRunOutput(null);
      setNotice(
        saved.test_status.status === "passed"
          ? `Skill "${saved.name}" v${saved.version} saved — test passed.`
          : `Skill "${saved.name}" v${saved.version} saved but flagged: ${saved.test_status.status === "failed" ? saved.test_status.detail : ""}`,
      );
    } catch (e) {
      setNotice(String(e));
    }
  };

  const retest = async (name: string) => {
    try {
      await testSkill(name);
      refresh();
    } catch (e) {
      setNotice(String(e));
    }
  };

  const execute = async () => {
    if (!current) return;
    setRunOutput(null);
    try {
      setRunOutput(await runSkill(current.name, runInput));
    } catch (e) {
      setRunOutput(String(e));
    }
  };

  return (
    <div className="notes-view">
      <aside className="notes-list">
        <div className="panel-title-row">
          <span className="panel-title">skills</span>
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
        {skills.length === 0 && !creating && (
          <p className="panel-hint">
            No skills yet. Each skill is code plus a test — failing skills are
            flagged and refuse to run.
          </p>
        )}
        <ul>
          {skills.map((s) => (
            <li key={s.name}>
              <button
                type="button"
                className="note-item skill-item"
                data-active={s.name === selected}
                onClick={() => {
                  setSelected(s.name);
                  setCreating(false);
                  setRunOutput(null);
                }}
              >
                <span>
                  {s.name} <span className="skill-version">v{s.version}</span>
                </span>
                <span className="skill-badge" data-state={s.test_status.status}>
                  {testBadge(s.test_status)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </aside>

      <section className="notes-body">
        {notice && (
          <div className="msg" data-role="system">
            {notice}
          </div>
        )}
        {creating ? (
          <div className="note-editor">
            <input
              className="chat-input"
              value={draftName}
              placeholder="Skill name"
              aria-label="skill name"
              onChange={(e) => setDraftName(e.target.value)}
            />
            <input
              className="chat-input"
              value={draftDesc}
              placeholder="What does it do?"
              aria-label="skill description"
              onChange={(e) => setDraftDesc(e.target.value)}
            />
            <label className="panel-title" htmlFor="skill-code">
              code · fn run(input)
            </label>
            <textarea
              id="skill-code"
              className="chat-input note-textarea code-area"
              value={draftCode}
              spellCheck={false}
              onChange={(e) => setDraftCode(e.target.value)}
            />
            <label className="panel-title" htmlFor="skill-test">
              bundled test · fn test()
            </label>
            <textarea
              id="skill-test"
              className="chat-input note-textarea code-area"
              value={draftTest}
              spellCheck={false}
              onChange={(e) => setDraftTest(e.target.value)}
            />
            <div className="editor-actions">
              <button
                type="button"
                className="send-btn"
                disabled={!draftName.trim()}
                onClick={() => void create()}
              >
                Save & test
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
        ) : current ? (
          <div className="skill-detail">
            <div className="panel-title-row">
              <span className="panel-title">
                {current.name} v{current.version} ·{" "}
                {testBadge(current.test_status)}
              </span>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => void retest(current.name)}
              >
                Re-run test
              </button>
            </div>
            {current.description && (
              <p className="panel-hint">{current.description}</p>
            )}
            {current.test_status.status === "failed" && (
              <div className="msg" data-role="system">
                Flagged: {current.test_status.detail}
              </div>
            )}
            <div className="composer">
              <input
                className="chat-input"
                value={runInput}
                placeholder="Input for run(input)…"
                aria-label="skill input"
                onChange={(e) => setRunInput(e.target.value)}
              />
              <button
                type="button"
                className="send-btn"
                disabled={current.test_status.status !== "passed"}
                onClick={() => void execute()}
              >
                Run
              </button>
            </div>
            {runOutput !== null && (
              <pre className="note-content skill-output">{runOutput}</pre>
            )}
          </div>
        ) : (
          <div className="empty-state">
            <h1>Skill library</h1>
            <p>
              Skills compound: each one is saved, versioned, and tested. Select
              one, or create the first.
            </p>
          </div>
        )}
      </section>
    </div>
  );
}
