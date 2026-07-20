//! Thin Tauri adapter over the core modules. All real logic lives in
//! `core/` (Tauri-free and unit-tested); commands here only translate
//! between IPC and the core types.

pub mod core;

use crate::core::authoring::{
    authoring_messages, parse_skill_draft, refinement_message, MAX_ATTEMPTS,
};
use crate::core::confidence::{confidence_instruction, extract_confidence};
use crate::core::eventlog::{Event, EventLog};
use crate::core::memory::{Insight, MemoryStore, StoredMessage};
use crate::core::reflection::{
    digest_events, parse_insights, reflection_messages, with_lessons, INSIGHTS_IN_PROMPT,
    REFLECT_EVERY_MESSAGES,
};
use crate::core::replay::{audit, rebuild_messages, ReplayReport, ReplayedMessage};
use crate::core::router::{onboarding_message, ChatMessage, ChatReply, Router, RouterConfig};
use crate::core::skills::{SkillEngine, SkillManifest};
use crate::core::tools::NotesTool;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use tauri::Manager;

struct AppState {
    memory: Mutex<MemoryStore>,
    router: Router,
    notes: NotesTool,
    skills: SkillEngine,
    events: Mutex<EventLog>,
    system: Mutex<sysinfo::System>,
    started: Instant,
}

/// Best-effort append; the log must never take the assistant down with it.
fn log_event(state: &AppState, kind: &str, payload: serde_json::Value) {
    if let Ok(mut events) = state.events.lock() {
        let _ = events.append(kind, payload);
    }
}

#[derive(serde::Serialize)]
struct ProviderStatus {
    id: String,
    configured: bool,
    reachable: Option<bool>,
    model: String,
}

#[derive(serde::Serialize)]
struct Status {
    providers: Vec<ProviderStatus>,
    ready: bool,
    onboarding: Option<String>,
    message_count: u64,
    fact_count: u64,
}

#[derive(serde::Serialize)]
struct Telemetry {
    cpu_percent: f32,
    mem_used: u64,
    mem_total: u64,
    uptime_secs: u64,
    note_count: usize,
    message_count: u64,
    fact_count: u64,
}

const SYSTEM_PROMPT: &str = "You are H.O.T-Jarvis, a calm, capable, local-first personal \
assistant. Be concise and honest. You currently have one tool available to the user (local \
notes) and a persistent memory of this conversation. If you are unsure of something, say so \
plainly instead of guessing.";

#[tauri::command]
async fn get_status(state: tauri::State<'_, AppState>) -> Result<Status, String> {
    let ollama_ok = state.router.ollama_reachable().await;
    let cfg = state.router.config();
    let providers = vec![
        ProviderStatus {
            id: "ollama".into(),
            configured: true,
            reachable: Some(ollama_ok),
            model: cfg.ollama_model.clone(),
        },
        ProviderStatus {
            id: "groq".into(),
            configured: cfg.groq_api_key.is_some(),
            reachable: None,
            model: cfg.groq_model.clone(),
        },
        ProviderStatus {
            id: "openrouter".into(),
            configured: cfg.openrouter_api_key.is_some(),
            reachable: None,
            model: cfg.openrouter_model.clone(),
        },
    ];
    let ready = ollama_ok || cfg.groq_api_key.is_some() || cfg.openrouter_api_key.is_some();
    let (message_count, fact_count) = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        (
            mem.message_count().map_err(|e| e.to_string())?,
            mem.fact_count().map_err(|e| e.to_string())?,
        )
    };
    Ok(Status {
        providers,
        ready,
        onboarding: if ready {
            None
        } else {
            Some(onboarding_message())
        },
        message_count,
        fact_count,
    })
}

#[tauri::command]
async fn chat_send(state: tauri::State<'_, AppState>, text: String) -> Result<ChatReply, String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err("empty message".into());
    }
    // Persist the user turn and build context, releasing the lock before I/O.
    let mut context = Vec::new();
    let user_msg_id;
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        user_msg_id = mem
            .append_message("user", &trimmed)
            .map_err(|e| e.to_string())?;
        let lessons: Vec<String> = mem
            .recent_insights(INSIGHTS_IN_PROMPT)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|i| i.content)
            .collect();
        context.push(ChatMessage {
            role: "system".into(),
            content: format!(
                "{}{}",
                with_lessons(SYSTEM_PROMPT, &lessons),
                confidence_instruction()
            ),
        });
        for m in mem.recent_messages(20).map_err(|e| e.to_string())? {
            context.push(ChatMessage {
                role: m.role,
                content: m.content,
            });
        }
    }
    log_event(
        &state,
        "chat.user",
        serde_json::json!({ "text": trimmed, "msg_id": user_msg_id }),
    );
    let asked_at = Instant::now();
    let outcome = state.router.chat(&context).await;
    match outcome {
        Ok(mut reply) => {
            // §5.3: pull the self-rating out of the text; it travels as data.
            let (cleaned, confidence) = extract_confidence(&reply.content);
            reply.content = cleaned;
            reply.confidence = confidence;
            let assistant_msg_id = {
                let mem = state.memory.lock().map_err(|e| e.to_string())?;
                mem.append_message("assistant", &reply.content)
                    .map_err(|e| e.to_string())?
            };
            log_event(
                &state,
                "chat.assistant",
                serde_json::json!({
                    "text": reply.content,
                    "provider": reply.provider,
                    "model": reply.model,
                    "duration_ms": asked_at.elapsed().as_millis() as u64,
                    "confidence": reply.confidence,
                    "msg_id": assistant_msg_id,
                }),
            );
            Ok(reply)
        }
        Err(e) => {
            log_event(
                &state,
                "chat.failed",
                serde_json::json!({ "error": e.to_string() }),
            );
            Err(e.to_string())
        }
    }
}

#[tauri::command]
fn get_history(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<StoredMessage>, String> {
    let mem = state.memory.lock().map_err(|e| e.to_string())?;
    mem.recent_messages(limit.unwrap_or(200) as usize)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_note(
    state: tauri::State<'_, AppState>,
    title: String,
    content: String,
) -> Result<String, String> {
    // Capture the inverse state up front: undo needs to know what was there.
    let previous = state.notes.read_note(&title).ok();
    let slug = state
        .notes
        .save_note(&title, &content)
        .map_err(|e| e.to_string())?;
    log_event(
        &state,
        "note.saved",
        serde_json::json!({ "slug": slug, "chars": content.len(), "previous": previous }),
    );
    Ok(slug)
}

#[tauri::command]
fn list_notes(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    state.notes.list_notes().map_err(|e| e.to_string())
}

#[tauri::command]
fn read_note(state: tauri::State<'_, AppState>, name: String) -> Result<String, String> {
    state.notes.read_note(&name).map_err(|e| e.to_string())
}

/// Real machine and app vitals for the HUD's live readouts. First call
/// reports 0% CPU (sysinfo needs a prior sample); it settles by the next poll.
#[tauri::command]
fn get_telemetry(state: tauri::State<'_, AppState>) -> Result<Telemetry, String> {
    let (cpu_percent, mem_used, mem_total) = {
        let mut sys = state.system.lock().map_err(|e| e.to_string())?;
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        (
            sys.global_cpu_usage(),
            sys.used_memory(),
            sys.total_memory(),
        )
    };
    let note_count = state.notes.list_notes().map_err(|e| e.to_string())?.len();
    let (message_count, fact_count) = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        (
            mem.message_count().map_err(|e| e.to_string())?,
            mem.fact_count().map_err(|e| e.to_string())?,
        )
    };
    Ok(Telemetry {
        cpu_percent,
        mem_used,
        mem_total,
        uptime_secs: state.started.elapsed().as_secs(),
        note_count,
        message_count,
        fact_count,
    })
}

#[tauri::command]
fn save_skill(
    state: tauri::State<'_, AppState>,
    name: String,
    description: String,
    code: String,
    test: String,
) -> Result<SkillManifest, String> {
    let manifest = state
        .skills
        .save_skill(&name, &description, &code, &test)
        .map_err(|e| e.to_string())?;
    log_event(
        &state,
        "skill.saved",
        serde_json::json!({
            "name": manifest.name,
            "version": manifest.version,
            "test_status": manifest.test_status,
        }),
    );
    Ok(manifest)
}

#[derive(serde::Serialize)]
struct AuthoringOutcome {
    manifest: SkillManifest,
    attempts: u32,
    passed: bool,
}

/// "Jarvis, learn to do X": the model drafts code + test, the engine
/// validates by running the test, and failures loop back to the model
/// with the error for up to MAX_ATTEMPTS rounds. The final draft is saved
/// either way — a failing skill lands flagged, visible, and refusable.
#[tauri::command]
async fn author_skill(
    state: tauri::State<'_, AppState>,
    request: String,
) -> Result<AuthoringOutcome, String> {
    let trimmed = request.trim().to_string();
    if trimmed.is_empty() {
        return Err("describe what the skill should do".into());
    }
    let skill_lessons: Vec<String> = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.recent_insights(10)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|i| i.kind == "skill")
            .take(INSIGHTS_IN_PROMPT)
            .map(|i| i.content)
            .collect()
    };
    let mut conversation = authoring_messages(&trimmed, &skill_lessons);
    let mut last_error = String::new();
    let mut saved: Option<SkillManifest> = None;

    for attempt in 1..=MAX_ATTEMPTS {
        let reply = state
            .router
            .chat(&conversation)
            .await
            .map_err(|e| e.to_string())?;
        conversation.push(ChatMessage {
            role: "assistant".into(),
            content: reply.content.clone(),
        });

        match parse_skill_draft(&reply.content) {
            Ok(draft) => {
                let manifest = state
                    .skills
                    .save_skill(&draft.name, &draft.description, &draft.code, &draft.test)
                    .map_err(|e| e.to_string())?;
                let passed = matches!(
                    manifest.test_status,
                    crate::core::skills::TestStatus::Passed
                );
                log_event(
                    &state,
                    "skill.authored",
                    serde_json::json!({
                        "name": manifest.name,
                        "version": manifest.version,
                        "attempt": attempt,
                        "test_status": manifest.test_status,
                        "request": trimmed,
                    }),
                );
                if passed {
                    return Ok(AuthoringOutcome {
                        manifest,
                        attempts: attempt,
                        passed: true,
                    });
                }
                last_error = match &manifest.test_status {
                    crate::core::skills::TestStatus::Failed(detail) => detail.clone(),
                    _ => "unknown failure".into(),
                };
                saved = Some(manifest);
            }
            Err(parse_error) => {
                last_error = parse_error;
            }
        }
        conversation.push(refinement_message(&last_error));
    }

    match saved {
        // Out of attempts: report the flagged skill honestly.
        Some(manifest) => Ok(AuthoringOutcome {
            manifest,
            attempts: MAX_ATTEMPTS,
            passed: false,
        }),
        None => Err(format!(
            "the model never produced a usable skill draft (last error: {last_error})"
        )),
    }
}

#[tauri::command]
fn list_skills(state: tauri::State<'_, AppState>) -> Result<Vec<SkillManifest>, String> {
    state.skills.list_skills().map_err(|e| e.to_string())
}

#[tauri::command]
fn test_skill(state: tauri::State<'_, AppState>, name: String) -> Result<SkillManifest, String> {
    let manifest = state.skills.test_skill(&name).map_err(|e| e.to_string())?;
    log_event(
        &state,
        "skill.tested",
        serde_json::json!({ "name": manifest.name, "test_status": manifest.test_status }),
    );
    Ok(manifest)
}

#[tauri::command]
fn run_skill(
    state: tauri::State<'_, AppState>,
    name: String,
    input: String,
) -> Result<String, String> {
    match state.skills.run_skill(&name, &input) {
        Ok(output) => {
            log_event(
                &state,
                "skill.run",
                serde_json::json!({ "name": name, "ok": true }),
            );
            Ok(output)
        }
        Err(e) => {
            log_event(
                &state,
                "skill.run",
                serde_json::json!({ "name": name, "ok": false, "error": e.to_string() }),
            );
            Err(e.to_string())
        }
    }
}

/// Reflection pass (§5.2): digest the events since the last pass, ask the
/// model for lessons, store them as insights. Returns the new insights.
async fn run_reflection(state: &AppState) -> Result<Vec<Insight>, String> {
    // Gather fresh events past the watermark; hold no lock across awaits.
    let (fresh, watermark) = {
        let last: u64 = {
            let mem = state.memory.lock().map_err(|e| e.to_string())?;
            mem.get_fact("reflection.last_event_id")
                .map_err(|e| e.to_string())?
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        };
        let events = {
            let log = state.events.lock().map_err(|e| e.to_string())?;
            log.tail(300).map_err(|e| e.to_string())?
        };
        let fresh: Vec<Event> = events.into_iter().filter(|e| e.id > last).collect();
        let watermark = fresh.iter().map(|e| e.id).max().unwrap_or(last);
        (fresh, watermark)
    };
    if fresh.is_empty() {
        return Ok(Vec::new());
    }

    let digest = digest_events(&fresh);
    let reply = state
        .router
        .chat(&reflection_messages(&digest))
        .await
        .map_err(|e| e.to_string())?;
    let drafts = parse_insights(&reply.content).unwrap_or_default();

    let mut stored = Vec::new();
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let source = format!("events ..{watermark}");
        for draft in &drafts {
            let id = mem
                .add_insight(&draft.kind, &draft.content, &source)
                .map_err(|e| e.to_string())?;
            stored.push(Insight {
                id,
                kind: draft.kind.clone(),
                content: draft.content.clone(),
                source: source.clone(),
                created_at: 0,
            });
        }
        // Advance the watermark even on an empty harvest so the same events
        // aren't re-digested forever.
        mem.set_fact("reflection.last_event_id", &watermark.to_string())
            .map_err(|e| e.to_string())?;
        let count = mem.message_count().map_err(|e| e.to_string())?;
        mem.set_fact("reflection.last_message_count", &count.to_string())
            .map_err(|e| e.to_string())?;
    }
    log_event(
        state,
        "memory.reflected",
        serde_json::json!({ "insights": stored.len(), "events_digested": fresh.len() }),
    );
    Ok(stored)
}

/// Manual "Reflect now" from the memory view.
#[tauri::command]
async fn reflect_now(state: tauri::State<'_, AppState>) -> Result<Vec<Insight>, String> {
    run_reflection(&state).await
}

/// Periodic trigger: the frontend calls this after chat turns; reflection
/// only actually runs once enough new conversation has accumulated.
#[tauri::command]
async fn reflect_if_due(state: tauri::State<'_, AppState>) -> Result<Option<usize>, String> {
    let due = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let count = mem.message_count().map_err(|e| e.to_string())?;
        let last: u64 = mem
            .get_fact("reflection.last_message_count")
            .map_err(|e| e.to_string())?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        count.saturating_sub(last) >= REFLECT_EVERY_MESSAGES
    };
    if !due {
        return Ok(None);
    }
    Ok(Some(run_reflection(&state).await?.len()))
}

#[tauri::command]
fn list_insights(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<Insight>, String> {
    let mem = state.memory.lock().map_err(|e| e.to_string())?;
    mem.recent_insights(limit.unwrap_or(50) as usize)
        .map_err(|e| e.to_string())
}

/// Undo (§5.4): reverses one recorded action using the inverse state
/// captured in its event. Every undo is itself an event — the timeline
/// never lies about what happened.
#[tauri::command]
fn undo_event(state: tauri::State<'_, AppState>, event_id: u64) -> Result<String, String> {
    let event = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|e| e.id == event_id)
            .ok_or_else(|| format!("event #{event_id} not found"))?
    };
    let p = &event.payload;
    let outcome = match event.kind.as_str() {
        "chat.user" | "chat.assistant" => {
            let msg_id = p["msg_id"]
                .as_i64()
                .ok_or("this message predates undo support")?;
            let removed = {
                let mem = state.memory.lock().map_err(|e| e.to_string())?;
                mem.delete_message(msg_id).map_err(|e| e.to_string())?
            };
            if !removed {
                return Err("that message is already gone".into());
            }
            log_event(
                &state,
                "undo.chat",
                serde_json::json!({ "undoes": event.id, "msg_id": msg_id }),
            );
            "message removed from memory".to_string()
        }
        "note.saved" => {
            let slug = p["slug"].as_str().ok_or("event has no note slug")?;
            let outcome = match p["previous"].as_str() {
                Some(previous) => {
                    state
                        .notes
                        .save_note(slug, previous)
                        .map_err(|e| e.to_string())?;
                    format!("note \"{slug}\" restored to its previous content")
                }
                None => {
                    state.notes.delete_note(slug).map_err(|e| e.to_string())?;
                    format!("note \"{slug}\" deleted (it was newly created)")
                }
            };
            log_event(
                &state,
                "undo.note",
                serde_json::json!({ "undoes": event.id, "slug": slug }),
            );
            outcome
        }
        "skill.saved" | "skill.authored" => {
            let name = p["name"].as_str().ok_or("event has no skill name")?;
            let rolled = state
                .skills
                .rollback_skill(name)
                .map_err(|e| e.to_string())?;
            let outcome = match &rolled {
                Some(manifest) => format!(
                    "skill \"{}\" reverted to previous behavior (as v{})",
                    name, manifest.version
                ),
                None => format!("skill \"{name}\" deleted (it had no previous version)"),
            };
            log_event(
                &state,
                "undo.skill",
                serde_json::json!({ "undoes": event.id, "name": name, "deleted": rolled.is_none() }),
            );
            outcome
        }
        other => {
            return Err(format!(
                "\"{other}\" actions aren't reversible (wipes and reflections are permanent)"
            ))
        }
    };
    Ok(outcome)
}

/// Replay audit (§5.4): rebuilds the conversation from the event log alone
/// and diffs it against the live database. Deterministic = they agree.
#[tauri::command]
fn replay_audit(state: tauri::State<'_, AppState>) -> Result<ReplayReport, String> {
    let events = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2).map_err(|e| e.to_string())?
    };
    let replayed = rebuild_messages(&events);
    let actual: Vec<ReplayedMessage> = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.recent_messages(usize::MAX / 2)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|m| ReplayedMessage {
                role: m.role,
                content: m.content,
            })
            .collect()
    };
    let report = audit(&replayed, &actual);
    log_event(
        &state,
        "replay.audited",
        serde_json::json!({
            "matched": report.matched,
            "missing_in_db": report.missing_in_db.len(),
            "extra_in_db": report.extra_in_db.len(),
            "deterministic": report.deterministic,
        }),
    );
    Ok(report)
}

/// Everything the assistant knows, in one JSON file: structured memory,
/// the full event log, and the notes. The user's data is theirs to take.
#[tauri::command]
fn export_memory(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mut dump = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.export_json().map_err(|e| e.to_string())?
    };
    let events = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2).map_err(|e| e.to_string())?
    };
    let notes: Vec<serde_json::Value> = state
        .notes
        .list_notes()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|name| {
            state
                .notes
                .read_note(&name)
                .ok()
                .map(|content| serde_json::json!({ "name": name, "content": content }))
        })
        .collect();
    dump["events"] = serde_json::to_value(events).map_err(|e| e.to_string())?;
    dump["notes"] = serde_json::Value::Array(notes);
    Ok(dump)
}

/// Wipes structured memory AND the event log (chat text lives there too).
/// Notes are documents, not memory — they stay until deleted explicitly.
#[tauri::command]
fn wipe_memory(state: tauri::State<'_, AppState>) -> Result<(), String> {
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.wipe().map_err(|e| e.to_string())?;
    }
    {
        let mut events = state.events.lock().map_err(|e| e.to_string())?;
        events.wipe().map_err(|e| e.to_string())?;
    }
    log_event(&state, "memory.wiped", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
fn get_events(state: tauri::State<'_, AppState>, limit: Option<u32>) -> Result<Vec<Event>, String> {
    let events = state.events.lock().map_err(|e| e.to_string())?;
    events
        .tail(limit.unwrap_or(200) as usize)
        .map_err(|e| e.to_string())
}

fn resolve_data_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    match std::env::var("JARVIS_DATA_DIR") {
        Ok(dir) if !dir.trim().is_empty() => Ok(PathBuf::from(dir.trim())),
        _ => Ok(app.path().app_data_dir()?),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Dev convenience: pick up a repo-root .env; harmless if absent.
    let _ = dotenvy::dotenv();
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = resolve_data_dir(app)?;
            let memory = MemoryStore::open(&data_dir.join("jarvis.sqlite3"))?;
            let router = Router::new(RouterConfig::from_env());
            let notes = NotesTool::new(&data_dir);
            let skills = SkillEngine::new(&data_dir);
            let mut events = EventLog::open(&data_dir.join("events.jsonl"))?;
            let _ = events.append(
                "app.started",
                serde_json::json!({ "version": app.package_info().version.to_string() }),
            );
            app.manage(AppState {
                memory: Mutex::new(memory),
                router,
                notes,
                skills,
                events: Mutex::new(events),
                system: Mutex::new(sysinfo::System::new()),
                started: Instant::now(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            chat_send,
            get_history,
            get_telemetry,
            get_events,
            save_note,
            list_notes,
            read_note,
            save_skill,
            author_skill,
            list_skills,
            test_skill,
            run_skill,
            reflect_now,
            reflect_if_due,
            list_insights,
            undo_event,
            replay_audit,
            export_memory,
            wipe_memory
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
