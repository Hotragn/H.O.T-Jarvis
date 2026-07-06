//! Thin Tauri adapter over the core modules. All real logic lives in
//! `core/` (Tauri-free and unit-tested); commands here only translate
//! between IPC and the core types.

pub mod core;

use crate::core::eventlog::{Event, EventLog};
use crate::core::memory::{MemoryStore, StoredMessage};
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
    let mut context = vec![ChatMessage {
        role: "system".into(),
        content: SYSTEM_PROMPT.into(),
    }];
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.append_message("user", &trimmed)
            .map_err(|e| e.to_string())?;
        for m in mem.recent_messages(20).map_err(|e| e.to_string())? {
            context.push(ChatMessage {
                role: m.role,
                content: m.content,
            });
        }
    }
    log_event(&state, "chat.user", serde_json::json!({ "text": trimmed }));
    let asked_at = Instant::now();
    let outcome = state.router.chat(&context).await;
    match outcome {
        Ok(reply) => {
            {
                let mem = state.memory.lock().map_err(|e| e.to_string())?;
                mem.append_message("assistant", &reply.content)
                    .map_err(|e| e.to_string())?;
            }
            log_event(
                &state,
                "chat.assistant",
                serde_json::json!({
                    "text": reply.content,
                    "provider": reply.provider,
                    "model": reply.model,
                    "duration_ms": asked_at.elapsed().as_millis() as u64,
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
    let slug = state
        .notes
        .save_note(&title, &content)
        .map_err(|e| e.to_string())?;
    log_event(
        &state,
        "note.saved",
        serde_json::json!({ "slug": slug, "chars": content.len() }),
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

#[tauri::command]
fn export_memory(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mem = state.memory.lock().map_err(|e| e.to_string())?;
    mem.export_json().map_err(|e| e.to_string())
}

#[tauri::command]
fn wipe_memory(state: tauri::State<'_, AppState>) -> Result<(), String> {
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.wipe().map_err(|e| e.to_string())?;
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
            list_skills,
            test_skill,
            run_skill,
            export_memory,
            wipe_memory
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
