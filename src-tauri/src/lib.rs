//! Thin Tauri adapter over the core modules. All real logic lives in
//! `core/` (Tauri-free and unit-tested); commands here only translate
//! between IPC and the core types.

pub mod core;
#[cfg(desktop)]
pub mod mic;

use crate::core::authoring::{
    authoring_messages, parse_skill_draft, refinement_message, MAX_ATTEMPTS,
};
use crate::core::confidence::{confidence_instruction, extract_confidence};
use crate::core::embedding;
use crate::core::eventlog::{Event, EventLog};
use crate::core::forgetting;
use crate::core::memory::{Insight, MemoryStore, StoredMessage};
use crate::core::reflection::{
    digest_events, parse_insights, reflection_messages, with_lessons, INSIGHTS_IN_PROMPT,
    REFLECT_EVERY_MESSAGES,
};
use crate::core::replay::{audit, rebuild_messages, ReplayReport, ReplayedMessage};
use crate::core::router::{onboarding_message, ChatMessage, ChatReply, Router, RouterConfig};
use crate::core::skills::{SkillEngine, SkillManifest};
use crate::core::stt::{self, SttReadiness};
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
    /// Where models and databases live; needed to find cached STT weights.
    data_dir: PathBuf,
    /// Push-to-talk capture. One take at a time. Desktop-only: cpal isn't built
    /// for mobile targets.
    #[cfg(desktop)]
    recorder: crate::mic::MicRecorder,
    /// The loaded Whisper model, kept warm between takes (loading costs seconds).
    #[cfg(feature = "local-whisper")]
    transcriber: Mutex<Option<crate::core::whisper::WhisperTranscriber>>,
    /// Voice v2: the hands-free conversation session. Its phase decides whether
    /// the mic should be open, so the whole policy lives in one tested place.
    #[cfg(desktop)]
    session: Mutex<crate::core::conversation::Session>,
}

/// The live calibration report, rebuilt from the event log. Cheap enough to do
/// per chat turn at personal-history scale, and always current.
fn current_calibration(state: &AppState) -> crate::core::calibration::CalibrationReport {
    let events = state
        .events
        .lock()
        .ok()
        .and_then(|log| log.tail(usize::MAX / 2).ok())
        .unwrap_or_default();
    crate::core::calibration::report(&crate::core::calibration::pair_from_events(&events))
}

/// Wall-clock seconds, for scoring lesson age.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
    // The user is here; unattended work should wait (idle gate, §7).
    touch_user_activity(&state);
    // Persist the user turn and gather context, releasing the lock before I/O.
    let user_msg_id;
    let base_system;
    let recent;
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        user_msg_id = mem
            .append_message("user", &trimmed)
            .map_err(|e| e.to_string())?;
        // Reflection v1: pick by score, not recency. A corroborated lesson beats
        // a fresh guess, and a stale one fades out of the prompt on its own.
        let pool = mem.recent_insights(300).map_err(|e| e.to_string())?;
        let candidates: Vec<forgetting::Candidate> = pool
            .iter()
            .map(|i| forgetting::Candidate {
                id: i.id,
                kind: i.kind.clone(),
                content: i.content.clone(),
                created_at: i.created_at,
                corroborations: i.corroborations,
                uses: i.uses,
                // Scoring for prompt selection is arithmetic on the
                // bookkeeping; only duplicate detection needs vectors.
                embedding: None,
            })
            .collect();
        let picked = forgetting::top_for_prompt(&candidates, now_unix(), INSIGHTS_IN_PROMPT);
        let used_ids: Vec<i64> = picked.iter().map(|c| c.id).collect();
        let lessons: Vec<String> = picked.iter().map(|c| c.content.clone()).collect();
        // Using a lesson is itself a weak signal that it's relevant.
        let _ = mem.mark_insights_used(&used_ids);
        base_system = format!(
            "{}{}",
            with_lessons(SYSTEM_PROMPT, &lessons),
            confidence_instruction()
        );
        recent = mem.recent_messages(20).map_err(|e| e.to_string())?;
    }

    // Confidence v2: tell the model what its own record says, so it can correct
    // a measured bias instead of repeating it.
    let calibration = current_calibration(&state);
    let base_system = format!(
        "{base_system}{}",
        crate::core::confidence::bias_instruction(&calibration)
    );

    // Semantic recall: fish the archive for moments the recent window has
    // already forgotten. Strictly best-effort — no embedding model means no
    // recall, never an error — and local-only (embeddings never leave for a
    // cloud provider, so recall can't quietly break the privacy promise).
    let recall_section = match state.router.embed(&trimmed).await {
        Ok(query_vec) => {
            let mem = state.memory.lock().map_err(|e| e.to_string())?;
            let vectors = mem
                .embeddings_for_model(&state.router.embed_model())
                .unwrap_or_default();
            let recent_ids: Vec<i64> = recent.iter().map(|m| m.id).collect();
            let hits = embedding::top_k(
                &query_vec,
                &vectors,
                embedding::RECALL_IN_PROMPT,
                embedding::RECALL_FLOOR,
                &recent_ids,
            );
            let ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
            let messages = mem.messages_by_ids(&ids).unwrap_or_default();
            embedding::recall_prompt_section(
                &messages
                    .into_iter()
                    .map(|m| (m.role, m.content))
                    .collect::<Vec<_>>(),
            )
        }
        Err(_) => String::new(),
    };

    let mut context = Vec::new();
    context.push(ChatMessage {
        role: "system".into(),
        content: format!("{base_system}{recall_section}"),
    });
    for m in recent {
        context.push(ChatMessage {
            role: m.role,
            content: m.content,
        });
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
            // Re-read the stated number through the calibration record: an
            // answer claiming 85 from a model that runs 30 points hot is not
            // an 85, and the user should know before acting on it.
            reply.trust = crate::core::confidence::assess(confidence, &calibration);
            let assistant_msg_id = {
                let mem = state.memory.lock().map_err(|e| e.to_string())?;
                mem.append_message("assistant", &reply.content)
                    .map_err(|e| e.to_string())?
            };
            // The UI grades this answer later; it needs the row id to do so.
            reply.msg_id = Some(assistant_msg_id);
            // Index both turns for future recall. Best-effort: a missing
            // embedding model just means these turns join the backfill list.
            for (msg_id, text) in [(user_msg_id, &trimmed), (assistant_msg_id, &reply.content)] {
                if let Ok(vector) = state.router.embed(text).await {
                    if let Ok(mem) = state.memory.lock() {
                        let _ = mem.upsert_embedding(msg_id, &state.router.embed_model(), &vector);
                    }
                }
            }
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

/// Deletes a note, capturing its content first so the deletion is undoable
/// from the timeline like every other destructive action.
#[tauri::command]
fn delete_note(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    let previous = state.notes.read_note(&name).map_err(|e| e.to_string())?;
    let removed = state.notes.delete_note(&name).map_err(|e| e.to_string())?;
    if !removed {
        return Err(format!("note \"{name}\" was already gone"));
    }
    log_event(
        &state,
        "note.deleted",
        serde_json::json!({ "slug": name, "previous": previous }),
    );
    Ok(())
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
        // Structured output: constrain the reply to the skill schema so the
        // model cannot emit fences or prose. This removes the "reply wasn't
        // JSON" failure class rather than recovering from it, which is what
        // leaves the retries for real logic mistakes.
        let reply = state
            .router
            .chat_json(&conversation, &crate::core::authoring::skill_schema())
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

    // Reflection v2: embed each draft before touching the database, so duplicate
    // detection can compare meaning rather than vocabulary. Best-effort by
    // design — no embedding model on disk gives `None`, which falls back to word
    // overlap instead of failing the whole pass.
    let mut draft_vectors: Vec<Option<Vec<f32>>> = Vec::with_capacity(drafts.len());
    for draft in &drafts {
        draft_vectors.push(state.router.embed(&draft.content).await.ok());
    }

    let mut stored: Vec<Insight> = Vec::new();
    let mut to_embed: Vec<(i64, Vec<f32>)> = Vec::new();
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let source = format!("events ..{watermark}");
        // Reflection v1: a lesson we already hold shouldn't be stored twice. An
        // independent re-derivation is evidence, so credit the existing copy
        // instead — that corroboration is what keeps it alive under decay.
        let existing = mem.recent_insights(500).map_err(|e| e.to_string())?;
        let known: Vec<forgetting::Candidate> = existing
            .iter()
            .map(|i| forgetting::Candidate {
                id: i.id,
                kind: i.kind.clone(),
                content: i.content.clone(),
                created_at: i.created_at,
                corroborations: i.corroborations,
                uses: i.uses,
                embedding: mem.insight_embedding(i.id).ok().flatten(),
            })
            .collect();
        for (draft, vector) in drafts.iter().zip(&draft_vectors) {
            let candidate = forgetting::Candidate {
                id: 0,
                kind: draft.kind.clone(),
                content: draft.content.clone(),
                created_at: now_unix(),
                corroborations: 0,
                uses: 0,
                embedding: vector.clone(),
            };
            if let Some(twin) = known
                .iter()
                .find(|k| forgetting::duplicate_match(k, &candidate).is_some())
            {
                mem.corroborate_insight(twin.id)
                    .map_err(|e| e.to_string())?;
                continue;
            }
            let id = mem
                .add_insight(&draft.kind, &draft.content, &source)
                .map_err(|e| e.to_string())?;
            if let Some(v) = vector {
                to_embed.push((id, v.clone()));
            }
            stored.push(Insight {
                id,
                kind: draft.kind.clone(),
                content: draft.content.clone(),
                source: source.clone(),
                created_at: 0,
                corroborations: 0,
                uses: 0,
                forgotten_at: None,
            });
        }
        // Store the vectors alongside the lessons they belong to. A failure here
        // costs recall quality on the next pass, never the lesson itself.
        for (id, vector) in &to_embed {
            let _ = mem.set_insight_embedding(*id, &state.router.embed_model(), vector);
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
        serde_json::json!({
            "insights": stored.len(),
            "events_digested": fresh.len(),
            // Ids make the pass replayable and undoable, rather than just counted.
            "insight_ids": stored.iter().map(|i| i.id).collect::<Vec<_>>(),
        }),
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
        "memory.reflected" => {
            // Reverses one reflection pass by dropping exactly the lessons it
            // created. Older passes logged only a count, so those stay
            // irreversible and say so.
            let ids: Vec<i64> = p["insight_ids"]
                .as_array()
                .ok_or("this reflection predates undo support")?
                .iter()
                .filter_map(|v| v.as_i64())
                .collect();
            if ids.is_empty() {
                return Err("that pass produced no lessons to undo".into());
            }
            {
                let mem = state.memory.lock().map_err(|e| e.to_string())?;
                for id in &ids {
                    mem.forget_insight(*id).map_err(|e| e.to_string())?;
                }
            }
            log_event(
                &state,
                "undo.reflection",
                serde_json::json!({ "undoes": event.id, "insight_ids": ids }),
            );
            format!("{} lesson(s) from that pass forgotten", ids.len())
        }
        "note.deleted" => {
            let slug = p["slug"].as_str().ok_or("event has no note slug")?;
            let previous = p["previous"]
                .as_str()
                .ok_or("this deletion predates undo support")?;
            state
                .notes
                .save_note(slug, previous)
                .map_err(|e| e.to_string())?;
            log_event(
                &state,
                "undo.note",
                serde_json::json!({ "undoes": event.id, "slug": slug }),
            );
            format!("note \"{slug}\" restored")
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

// --- M3: auto mode (§7) — guardrails first, then a cycle ---

use crate::core::autonomy;

/// Where the emergency stop file lives. Inside the app data dir so it works on
/// a locked-down machine, and documented so a user can create it by hand when
/// the UI is unresponsive — which is exactly when they'd need to.
fn stop_file_path(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join(".jarvis").join("STOP")
}

fn autonomy_enabled(mem: &MemoryStore) -> bool {
    mem.get_fact("autonomy.enabled")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false)
}

fn autonomy_caps(mem: &MemoryStore) -> autonomy::Caps {
    mem.get_fact("autonomy.caps")
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// When the user last did something in the app. Written on chat and voice, read
/// by the idle gate so unattended work never competes with a live conversation.
fn last_user_activity(mem: &MemoryStore) -> Option<i64> {
    mem.get_fact("autonomy.last_user_activity")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
}

/// Best-effort: a missed activity stamp only makes the loop more cautious, never
/// less, because a stale (older) stamp reads as "idle for longer".
fn touch_user_activity(state: &AppState) {
    if let Ok(mem) = state.memory.lock() {
        let _ = mem.set_fact("autonomy.last_user_activity", &now_unix().to_string());
    }
}

fn last_cycle_at(mem: &MemoryStore) -> Option<i64> {
    mem.get_fact("autonomy.last_cycle_at")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
}

/// Everything the UI needs to show auto mode honestly, including *why* it is
/// halted when it is.
#[derive(serde::Serialize)]
struct AutonomyStatus {
    enabled: bool,
    caps: autonomy::Caps,
    /// None when a cycle could run right now.
    halt: Option<autonomy::Halt>,
    stop_file: String,
    stop_file_exists: bool,
    last_cycle_at: Option<i64>,
}

fn autonomy_status(state: &AppState) -> Result<AutonomyStatus, String> {
    let stop = stop_file_path(&state.data_dir);
    let stop_file_exists = stop.exists();
    let mem = state.memory.lock().map_err(|e| e.to_string())?;
    let enabled = autonomy_enabled(&mem);
    let caps = autonomy_caps(&mem);
    let last = last_cycle_at(&mem);
    let active = last_user_activity(&mem);
    let env = std::env::var("JARVIS_AUTONOMY").ok();
    let halt = autonomy::may_start(
        enabled,
        stop_file_exists,
        env.as_deref(),
        last,
        active,
        now_unix(),
        &caps,
    )
    .err();
    Ok(AutonomyStatus {
        enabled,
        caps,
        halt,
        stop_file: stop.display().to_string(),
        stop_file_exists,
        last_cycle_at: last,
    })
}

#[tauri::command]
fn autonomy_state(state: tauri::State<'_, AppState>) -> Result<AutonomyStatus, String> {
    autonomy_status(&state)
}

/// Turns auto mode on or off. Off is always allowed; on is still subject to the
/// kill switch and caps at cycle time.
#[tauri::command]
fn autonomy_set_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<AutonomyStatus, String> {
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.set_fact("autonomy.enabled", if enabled { "true" } else { "false" })
            .map_err(|e| e.to_string())?;
    }
    log_event(
        &state,
        "autonomy.toggled",
        serde_json::json!({ "enabled": enabled }),
    );
    autonomy_status(&state)
}

#[tauri::command]
fn autonomy_set_caps(
    state: tauri::State<'_, AppState>,
    caps: autonomy::Caps,
) -> Result<AutonomyStatus, String> {
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.set_fact(
            "autonomy.caps",
            &serde_json::to_string(&caps).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    autonomy_status(&state)
}

/// Creates or removes the stop file. Creating it is the emergency brake and is
/// always permitted; removing it is how you rearm.
#[tauri::command]
fn autonomy_stop_file(
    state: tauri::State<'_, AppState>,
    engage: bool,
) -> Result<AutonomyStatus, String> {
    let path = stop_file_path(&state.data_dir);
    if engage {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            &path,
            "Auto mode is halted while this file exists. Delete it to rearm.\n",
        )
        .map_err(|e| e.to_string())?;
    } else if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    log_event(
        &state,
        "autonomy.stop_file",
        serde_json::json!({ "engaged": engage }),
    );
    autonomy_status(&state)
}

/// Reads the app's current shape for the planner.
fn autonomy_snapshot(state: &AppState) -> Result<autonomy::AppSnapshot, String> {
    let events = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2).map_err(|e| e.to_string())?
    };
    let mem = state.memory.lock().map_err(|e| e.to_string())?;
    let last_reflection: u64 = mem
        .get_fact("reflection.last_event_id")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let untested = state
        .skills
        .list_skills()
        .map(|list| {
            list.iter()
                .filter(|m| !matches!(m.test_status, crate::core::skills::TestStatus::Passed))
                .count() as u32
        })
        .unwrap_or(0);
    Ok(autonomy::AppSnapshot {
        unindexed_messages: mem
            .unembedded_message_ids(500)
            .map(|v| v.len() as u32)
            .unwrap_or(0),
        events_since_reflection: events.iter().filter(|e| e.id > last_reflection).count() as u32,
        insights: mem.insight_count().unwrap_or(0) as u32,
        untested_skills: untested,
    })
}

/// Dry run: what a cycle *would* do. Always available, even when halted — seeing
/// the plan is how you decide whether to arm it.
#[tauri::command]
fn autonomy_plan(state: tauri::State<'_, AppState>) -> Result<autonomy::CyclePlan, String> {
    let snap = autonomy_snapshot(&state)?;
    let caps = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        autonomy_caps(&mem)
    };
    Ok(autonomy::plan_cycle(&snap, &caps))
}

/// Only one cycle may execute at a time, whether it was started by the heartbeat
/// or by the button.
static CYCLE_GATE: autonomy::CycleGate = autonomy::CycleGate::new();

/// One place that turns a refusal into words, so the button and the heartbeat
/// never explain the same halt differently.
fn halt_message(halt: autonomy::Halt) -> String {
    match halt {
        autonomy::Halt::StopFile => {
            "auto mode is halted by the STOP file — delete it to rearm".to_string()
        }
        autonomy::Halt::EnvVar => "auto mode is disabled by JARVIS_AUTONOMY".to_string(),
        autonomy::Halt::Disabled => "auto mode is off".to_string(),
        autonomy::Halt::TooSoon { wait_secs } => format!("too soon — next cycle in {wait_secs}s"),
        autonomy::Halt::Busy { wait_secs } => {
            format!("you're using the app — unattended work waits {wait_secs}s")
        }
        autonomy::Halt::AlreadyRunning => "a cycle is already running".to_string(),
    }
}

/// Runs one cycle. Refuses unless every gate passes, re-checks the stop file
/// between actions, and logs what it did with its usage.
#[tauri::command]
async fn autonomy_run_cycle(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    // One cycle at a time, before anything else. The cycle gap is only written
    // when a cycle finishes, so mid-flight `may_start` still reads as satisfied:
    // the heartbeat and this command would otherwise both be told yes and
    // together spend twice the caps. Released on drop, including on a panic.
    let _in_flight = CYCLE_GATE.enter().map_err(halt_message)?;

    // Then the policy gate. `may_start` is the single place that decides this.
    let status = autonomy_status(&state)?;
    if let Some(halt) = status.halt {
        return Err(halt_message(halt));
    }

    let plan = {
        let snap = autonomy_snapshot(&state)?;
        autonomy::plan_cycle(&snap, &status.caps)
    };

    let started = Instant::now();
    let mut usage = autonomy::Usage::default();
    let mut done: Vec<serde_json::Value> = Vec::new();
    let mut stop_reason = plan.stop_reason;
    let stop_path = stop_file_path(&state.data_dir);

    for planned in &plan.actions {
        // The brake is checked between every action, not just at the start: a
        // cycle you can't interrupt isn't really interruptible.
        if stop_path.exists() {
            stop_reason = autonomy::StopReason::Killed;
            break;
        }
        let elapsed = started.elapsed().as_secs() as u32;
        usage.seconds = elapsed;
        if let Some(stop) = usage.room_for(&status.caps, planned.tool_calls) {
            stop_reason = stop;
            break;
        }

        // Belt and braces at the point of execution, not only at planning.
        if autonomy::classify(planned.action) != autonomy::Clearance::Auto {
            continue;
        }

        let outcome = match planned.action {
            autonomy::ActionKind::ReplayAudit => replay_audit_state(state.clone())
                .map(|r| serde_json::json!({ "deterministic": r.deterministic })),
            autonomy::ActionKind::TidyInsights => maintain_insights(state.clone())
                .map(|p| serde_json::json!({ "forgot": p.forget.len(), "kept": p.kept })),
            autonomy::ActionKind::IndexMemory => index_memory(state.clone())
                .await
                .map(|(indexed, remaining)| {
                    serde_json::json!({ "indexed": indexed, "remaining": remaining })
                }),
            autonomy::ActionKind::Reflect => reflect_now(state.clone())
                .await
                .map(|learned| serde_json::json!({ "insights": learned.len() })),
            autonomy::ActionKind::TestSkills => {
                // Verification only: re-run each skill's own bundled test.
                let names: Vec<String> = state
                    .skills
                    .list_skills()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|m| m.name)
                    .collect();
                let mut passed = 0usize;
                let mut failed = 0usize;
                for name in &names {
                    match state.skills.test_skill(name) {
                        Ok(m)
                            if matches!(
                                m.test_status,
                                crate::core::skills::TestStatus::Passed
                            ) =>
                        {
                            passed += 1
                        }
                        _ => failed += 1,
                    }
                }
                Ok(serde_json::json!({ "passed": passed, "failed": failed }))
            }
            // Unreachable: the classify check above already filtered these.
            _ => Err("action is not cleared for unattended work".to_string()),
        };

        usage.record(planned.tool_calls, started.elapsed().as_secs() as u32);
        done.push(serde_json::json!({
            "action": planned.action,
            "reason": planned.reason,
            "result": outcome.as_ref().ok(),
            "error": outcome.as_ref().err(),
        }));
    }

    usage.seconds = started.elapsed().as_secs() as u32;
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.set_fact("autonomy.last_cycle_at", &now_unix().to_string())
            .map_err(|e| e.to_string())?;
    }
    log_event(
        &state,
        "autonomy.cycle",
        serde_json::json!({
            "did": done,
            "usage": usage,
            "stop_reason": stop_reason,
            "deferred": plan.deferred.len(),
        }),
    );
    Ok(serde_json::json!({
        "did": done,
        "usage": usage,
        "stop_reason": stop_reason,
        "deferred": plan.deferred,
    }))
}

/// The last thing the heartbeat did, so the UI can prove it is alive. Kept in
/// memory only: it is diagnostics, not state worth persisting.
#[cfg(desktop)]
static LAST_BEAT: Mutex<Option<(i64, autonomy::Beat)>> = Mutex::new(None);

#[cfg(desktop)]
fn record_beat(beat: autonomy::Beat) {
    if let Ok(mut slot) = LAST_BEAT.lock() {
        *slot = Some((now_unix(), beat));
    }
}

/// What the heartbeat last did, and when.
#[tauri::command]
fn autonomy_last_beat() -> Option<serde_json::Value> {
    #[cfg(desktop)]
    {
        LAST_BEAT
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .map(|(at, beat)| serde_json::json!({ "at": at, "beat": beat }))
    }
    #[cfg(not(desktop))]
    {
        None
    }
}

/// Starts the background heartbeat (§7).
///
/// It wakes cheaply and often, and does nothing unless `may_start` says every
/// gate passes — so the STOP file stays responsive without the loop being a busy
/// loop. It never decides anything itself: the poll interval, the gates, and the
/// plan all come from the tested core.
#[cfg(desktop)]
fn spawn_heartbeat(app: tauri::AppHandle) {
    // A plain OS thread rather than an async task: the sleep is long and the one
    // async call is bridged with `block_on`, so there is nothing to gain from
    // holding a slot on the shared runtime.
    std::thread::spawn(move || {
        loop {
            // Re-read caps every wake so a change takes effect without a restart.
            let poll = {
                let state = app.state::<AppState>();
                let caps = state
                    .memory
                    .lock()
                    .map(|mem| autonomy_caps(&mem))
                    .unwrap_or_default();
                autonomy::heartbeat_poll_secs(&caps)
            };
            std::thread::sleep(std::time::Duration::from_secs(poll));

            // A cycle already running (started from the button) is a hold, not
            // an idle beat: reporting it as idle would misdescribe a busy loop.
            if CYCLE_GATE.is_running() {
                record_beat(autonomy::Beat::Held {
                    halt: autonomy::Halt::AlreadyRunning,
                });
                continue;
            }

            let state = app.state::<AppState>();
            // The gate decides. Nothing below runs unless it passes.
            match autonomy_status(&state) {
                Ok(status) => {
                    if let Some(halt) = status.halt {
                        record_beat(autonomy::Beat::Held { halt });
                        continue;
                    }
                }
                Err(_) => continue,
            }

            match tauri::async_runtime::block_on(autonomy_run_cycle(app.state::<AppState>())) {
                Ok(result) => {
                    let actions = result
                        .get("did")
                        .and_then(|d| d.as_array())
                        .map(|a| a.len() as u32)
                        .unwrap_or(0);
                    record_beat(if actions == 0 {
                        autonomy::Beat::Idle
                    } else {
                        autonomy::Beat::Ran { actions }
                    });
                }
                // A gate raced us between the check and the call. Report what it
                // actually said instead of guessing which gate it was.
                Err(why) => record_beat(autonomy::Beat::Refused { why }),
            }
        }
    });
}

// --- Voice v2: wake word + hands-free conversation (§6.4) ---

/// The saved wake phrase, or the default. Stored as a fact so it survives
/// restarts and can be changed on a phone with no env vars.
#[cfg(desktop)]
fn saved_wake_phrase(mem: &MemoryStore) -> String {
    mem.get_fact("voice.wake_phrase")
        .ok()
        .flatten()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| crate::core::hotword::DEFAULT_WAKE_PHRASE.to_string())
}

/// Current hands-free state, for the UI.
#[derive(serde::Serialize)]
struct VoiceSession {
    phase: crate::core::conversation::Phase,
    wake_phrase: String,
    /// Whether the mic should be open right now — the UI mirrors this rather
    /// than deciding for itself.
    wants_audio: bool,
    /// Whether the mic should be watching loudness (not transcribing) right now.
    /// Distinct from `wants_audio` on purpose: this is the barge-in monitor, and
    /// nothing it hears is ever sent to a model.
    wants_barge_monitor: bool,
    needs_wake: bool,
    follow_up_remaining_ms: u32,
}

#[cfg(desktop)]
fn snapshot(session: &crate::core::conversation::Session) -> VoiceSession {
    VoiceSession {
        phase: session.phase(),
        wake_phrase: session.wake_phrase().to_string(),
        wants_audio: session.phase().wants_audio(),
        wants_barge_monitor: session.phase().wants_barge_monitor(),
        needs_wake: session.phase().needs_wake(),
        follow_up_remaining_ms: session.follow_up_remaining_ms(),
    }
}

#[tauri::command]
fn voice_session(state: tauri::State<'_, AppState>) -> Result<VoiceSession, String> {
    #[cfg(desktop)]
    {
        let session = state.session.lock().map_err(|e| e.to_string())?;
        Ok(snapshot(&session))
    }
    #[cfg(not(desktop))]
    {
        let _ = state;
        Err(MOBILE_NO_CAPTURE.into())
    }
}

/// Turns hands-free on or off.
#[tauri::command]
fn voice_hands_free(state: tauri::State<'_, AppState>, on: bool) -> Result<VoiceSession, String> {
    #[cfg(desktop)]
    {
        let snap = {
            let mut session = state.session.lock().map_err(|e| e.to_string())?;
            if on {
                session.start();
            } else {
                session.stop();
                // Drop any take in flight so the mic actually closes.
                state.recorder.cancel();
            }
            snapshot(&session)
        };
        log_event(
            &state,
            "voice.hands_free",
            serde_json::json!({ "on": on, "phase": snap.phase }),
        );
        Ok(snap)
    }
    #[cfg(not(desktop))]
    {
        let _ = (state, on);
        Err(MOBILE_NO_CAPTURE.into())
    }
}

/// Sets the wake phrase (persisted).
#[tauri::command]
fn voice_set_wake_phrase(
    state: tauri::State<'_, AppState>,
    phrase: String,
) -> Result<VoiceSession, String> {
    #[cfg(desktop)]
    {
        let trimmed = phrase.trim();
        if trimmed.split_whitespace().count() < 2 {
            // One-word phrases fire on ordinary speech constantly.
            return Err("use at least two words, so it doesn't trigger by accident".into());
        }
        {
            let mem = state.memory.lock().map_err(|e| e.to_string())?;
            mem.set_fact("voice.wake_phrase", trimmed)
                .map_err(|e| e.to_string())?;
        }
        let mut session = state.session.lock().map_err(|e| e.to_string())?;
        let was_on = session.phase() != crate::core::conversation::Phase::Off;
        *session = crate::core::conversation::Session::new(trimmed);
        if was_on {
            session.start();
        }
        Ok(snapshot(&session))
    }
    #[cfg(not(desktop))]
    {
        let _ = (state, phrase);
        Err(MOBILE_NO_CAPTURE.into())
    }
}

/// Feeds one finished, transcribed take into the session and returns what to do
/// next. The audio loop lives in the UI (it already owns speech synthesis and
/// the chat call); this keeps every *decision* in the tested core.
#[tauri::command]
fn voice_heard(
    state: tauri::State<'_, AppState>,
    transcript: String,
    duration_ms: u32,
) -> Result<serde_json::Value, String> {
    #[cfg(desktop)]
    {
        touch_user_activity(&state);
        let (action, snap) = {
            let mut session = state.session.lock().map_err(|e| e.to_string())?;
            let action = session.heard(&transcript, duration_ms);
            (action, snapshot(&session))
        };
        if !matches!(action, crate::core::conversation::Action::Idle) {
            log_event(
                &state,
                "voice.hands_free_action",
                serde_json::json!({ "action": action, "phase": snap.phase }),
            );
        }
        Ok(serde_json::json!({ "action": action, "session": snap }))
    }
    #[cfg(not(desktop))]
    {
        let _ = (state, transcript, duration_ms);
        Err(MOBILE_NO_CAPTURE.into())
    }
}

/// Reports a lifecycle event back to the session: the model answered, speech
/// finished, the call failed, or time passed.
#[tauri::command]
fn voice_advance(
    state: tauri::State<'_, AppState>,
    event: String,
    elapsed_ms: Option<u32>,
) -> Result<VoiceSession, String> {
    #[cfg(desktop)]
    {
        let mut session = state.session.lock().map_err(|e| e.to_string())?;
        match event.as_str() {
            "answered_speaking" => session.answered(true),
            "answered_silent" => session.answered(false),
            "finished_speaking" => session.finished_speaking(),
            "failed" => session.failed(),
            // Voice v3: the user talked over the answer.
            "interrupted" => session.interrupted(),
            "tick" => session.tick(elapsed_ms.unwrap_or(0)),
            other => return Err(format!("unknown voice event '{other}'")),
        };
        Ok(snapshot(&session))
    }
    #[cfg(not(desktop))]
    {
        let _ = (state, event, elapsed_ms);
        Err(MOBILE_NO_CAPTURE.into())
    }
}

/// Watches for the user talking over the assistant, for at most `max_ms`
/// (Voice v3).
///
/// Returns true if someone interrupted. Loudness only: nothing captured on this
/// path is kept or transcribed, so the assistant still cannot hear its own voice
/// into a request — the property `Phase::wants_audio` protects. The echo level it
/// measured is logged so a room too loud for barge-in can be diagnosed rather
/// than just appearing broken.
#[tauri::command]
async fn voice_watch_barge(state: tauri::State<'_, AppState>, max_ms: u32) -> Result<bool, String> {
    #[cfg(desktop)]
    {
        // Bounded: a stuck watcher would hold the microphone open indefinitely,
        // which is the one thing an open mic must never do.
        let capped = max_ms.clamp(500, 120_000);
        let found = tauri::async_runtime::spawn_blocking(move || {
            crate::mic::watch_for_barge(std::time::Duration::from_millis(capped as u64))
        })
        .await
        .map_err(|e| e.to_string())??;

        if let Some(echo) = found {
            log_event(
                &state,
                "voice.barged_in",
                serde_json::json!({ "echo_level": echo }),
            );
        }
        Ok(found.is_some())
    }
    #[cfg(not(desktop))]
    {
        let _ = (state, max_ms);
        Err(MOBILE_NO_CAPTURE.into())
    }
}

/// Stops an in-flight barge-in watch (Voice v3).
///
/// Called when playback ends on its own. Without it the watcher would hold the
/// microphone for the rest of its length estimate, and the follow-up capture that
/// should start right after the answer would find the device busy.
#[tauri::command]
fn voice_stop_barge_watch() -> Result<(), String> {
    #[cfg(desktop)]
    crate::mic::cancel_barge_watch();
    Ok(())
}

// --- Confidence v1: calibration tracking (§5.3) ---

/// Grades one answer. This is the only new input calibration needs — the
/// confidence itself is already in the event log.
#[tauri::command]
fn rate_message(
    state: tauri::State<'_, AppState>,
    msg_id: i64,
    helpful: bool,
) -> Result<(), String> {
    log_event(
        &state,
        crate::core::calibration::RATED_EVENT,
        serde_json::json!({ "msg_id": msg_id, "helpful": helpful }),
    );
    Ok(())
}

/// Scores stated confidence against what actually happened, rebuilt from the log.
#[tauri::command]
fn calibration_report(
    state: tauri::State<'_, AppState>,
) -> Result<crate::core::calibration::CalibrationReport, String> {
    let events = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2).map_err(|e| e.to_string())?
    };
    let predictions = crate::core::calibration::pair_from_events(&events);
    Ok(crate::core::calibration::report(&predictions))
}

// --- Voice v1: on-device speech-to-text (§6.4) ---

/// What the user's build can actually do, so the UI never offers a mic button
/// that silently does nothing.
#[tauri::command]
fn stt_status(state: tauri::State<'_, AppState>) -> SttReadiness {
    let spec = stt::resolve_model(None);
    #[cfg(feature = "local-whisper")]
    {
        if stt::is_downloaded(&state.data_dir, spec) {
            SttReadiness::Ready {
                model: spec.id.to_string(),
            }
        } else {
            SttReadiness::NeedsDownload {
                model: spec.id.to_string(),
                approx_mb: spec.approx_mb,
            }
        }
    }
    #[cfg(not(feature = "local-whisper"))]
    {
        let _ = (state, spec);
        SttReadiness::NotCompiled
    }
}

/// The microphone the take will come from, for display. Mobile has no cpal
/// backend compiled in, so it honestly reports nothing rather than guessing.
#[tauri::command]
fn stt_device() -> Option<String> {
    #[cfg(desktop)]
    {
        crate::mic::default_input_name()
    }
    #[cfg(not(desktop))]
    {
        None
    }
}

/// Voice input is desktop-only for now: cpal is excluded from mobile builds, and
/// iOS will use a native audio path instead (docs/ios/README.md).
#[cfg(not(desktop))]
const MOBILE_NO_CAPTURE: &str = "voice input isn't available on this platform yet";

/// Fetches the model once, streaming each file to a `.partial` and renaming it
/// into place, so an interrupted download can never masquerade as a good one.
#[tauri::command]
async fn stt_download_model(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let spec = stt::resolve_model(None);
    let data_dir = state.data_dir.clone();
    if stt::is_downloaded(&data_dir, spec) {
        return Ok(spec.id.to_string());
    }
    let dir = stt::model_dir(&data_dir, spec);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let client = reqwest::Client::new();
    for (url, dest) in stt::required_files(&data_dir, spec) {
        if dest.exists() {
            continue;
        }
        let staged = stt::staging_path(&dest);
        let bytes = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("download failed for {url}: {e}"))?
            .error_for_status()
            .map_err(|e| format!("download failed for {url}: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("download failed for {url}: {e}"))?;
        if bytes.is_empty() {
            return Err(format!("{url} returned an empty file"));
        }
        std::fs::write(&staged, &bytes)
            .map_err(|e| format!("could not write {}: {e}", staged.display()))?;
        std::fs::rename(&staged, &dest)
            .map_err(|e| format!("could not finish {}: {e}", dest.display()))?;
    }
    log_event(
        &state,
        "voice.model_downloaded",
        serde_json::json!({ "model": spec.id, "approx_mb": spec.approx_mb }),
    );
    Ok(spec.id.to_string())
}

/// Hands-free capture: listen until the speaker stops, then transcribe.
///
/// One call per utterance, driven in a loop by the UI while the session wants
/// audio. Returns an empty transcript when the window passed in silence, which
/// is the common case while merely armed — the caller loops again rather than
/// treating it as an error.
///
/// Blocking work runs on a worker thread so the UI thread is never held.
#[tauri::command]
async fn stt_listen(
    state: tauri::State<'_, AppState>,
    max_wait_ms: Option<u32>,
    max_utterance_ms: Option<u32>,
) -> Result<serde_json::Value, String> {
    #[cfg(not(desktop))]
    {
        let _ = (state, max_wait_ms, max_utterance_ms);
        return Err(MOBILE_NO_CAPTURE.into());
    }
    #[cfg(desktop)]
    {
        let max_wait = std::time::Duration::from_millis(u64::from(max_wait_ms.unwrap_or(6_000)));
        let max_utterance =
            std::time::Duration::from_millis(u64::from(max_utterance_ms.unwrap_or(15_000)));

        // cpal capture is blocking; keep it off the async runtime's thread.
        let captured = tauri::async_runtime::spawn_blocking(move || {
            crate::mic::listen_until_endpoint(max_wait, max_utterance)
        })
        .await
        .map_err(|e| format!("capture task failed: {e}"))??;

        let Some(captured) = captured else {
            // Silence. Not an error — the caller loops.
            return Ok(serde_json::json!({ "transcript": "", "duration_ms": 0 }));
        };
        let duration_ms = (captured.duration_secs() * 1000.0) as u32;
        let pcm = captured.for_whisper();
        if pcm.is_empty()
            || !crate::core::audio::has_speech(&pcm, crate::core::audio::WHISPER_SAMPLE_RATE)
        {
            return Ok(serde_json::json!({ "transcript": "", "duration_ms": duration_ms }));
        }

        #[cfg(feature = "local-whisper")]
        {
            let spec = stt::resolve_model(None);
            if !stt::is_downloaded(&state.data_dir, spec) {
                return Err(format!(
                    "the {} speech model isn't downloaded yet (~{} MB)",
                    spec.id, spec.approx_mb
                ));
            }
            let mut slot = state.transcriber.lock().map_err(|e| e.to_string())?;
            if slot.is_none() {
                let dir = stt::model_dir(&state.data_dir, spec);
                *slot = Some(crate::core::whisper::WhisperTranscriber::load(&dir, spec)?);
            }
            let text = slot
                .as_mut()
                .expect("transcriber loaded above")
                .transcribe(&pcm)?;
            Ok(serde_json::json!({ "transcript": text, "duration_ms": duration_ms }))
        }
        #[cfg(not(feature = "local-whisper"))]
        {
            // `state` is only read by the whisper branch; keep the signature
            // stable across both builds rather than cfg-ing the parameter.
            let _ = state;
            Err(
                "hands-free needs the local speech model — rebuild with `--features local-whisper`"
                    .into(),
            )
        }
    }
}

/// Begins a push-to-talk take. Returns the negotiated capture format.
#[tauri::command]
fn stt_start(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    #[cfg(not(desktop))]
    {
        let _ = state;
        return Err(MOBILE_NO_CAPTURE.into());
    }
    #[cfg(desktop)]
    {
        let (sample_rate, channels) = state.recorder.start()?;
        Ok(serde_json::json!({ "sample_rate": sample_rate, "channels": channels }))
    }
}

/// Throws away the current take without transcribing. Safe to call when idle,
/// which is what lets the UI clean up unconditionally on unmount.
#[tauri::command]
fn stt_cancel(state: tauri::State<'_, AppState>) {
    #[cfg(desktop)]
    state.recorder.cancel();
    #[cfg(not(desktop))]
    let _ = state;
}

/// Ends the take and transcribes it locally. Returns the text (empty when the
/// user said nothing usable, which is not an error).
#[tauri::command]
fn stt_stop(state: tauri::State<'_, AppState>) -> Result<String, String> {
    #[cfg(not(desktop))]
    {
        let _ = state;
        return Err(MOBILE_NO_CAPTURE.into());
    }
    #[cfg(desktop)]
    {
        let captured = state.recorder.stop()?;
        let secs = captured.duration_secs();
        let pcm = captured.for_whisper();

        if pcm.is_empty()
            || !crate::core::audio::has_speech(&pcm, crate::core::audio::WHISPER_SAMPLE_RATE)
        {
            log_event(
                &state,
                "voice.heard_nothing",
                serde_json::json!({ "seconds": secs }),
            );
            return Ok(String::new());
        }

        #[cfg(feature = "local-whisper")]
        {
            let spec = stt::resolve_model(None);
            if !stt::is_downloaded(&state.data_dir, spec) {
                return Err(format!(
                    "the {} speech model isn't downloaded yet (~{} MB)",
                    spec.id, spec.approx_mb
                ));
            }
            let started = Instant::now();
            let mut slot = state.transcriber.lock().map_err(|e| e.to_string())?;
            if slot.is_none() {
                let dir = stt::model_dir(&state.data_dir, spec);
                *slot = Some(crate::core::whisper::WhisperTranscriber::load(&dir, spec)?);
            }
            let text = slot
                .as_mut()
                .expect("transcriber loaded above")
                .transcribe(&pcm)?;
            log_event(
                &state,
                "voice.transcribed",
                serde_json::json!({
                    "seconds": secs,
                    "chars": text.len(),
                    "model": spec.id,
                    "took_ms": started.elapsed().as_millis() as u64,
                }),
            );
            Ok(text)
        }
        #[cfg(not(feature = "local-whisper"))]
        {
            Err(
                "this build has no local speech model — rebuild with `--features local-whisper`"
                    .into(),
            )
        }
    }
}

/// Reflection v1: run a maintenance pass over the lesson store — collapse
/// duplicates into their established twin, and forget what has faded or been
/// squeezed out. Every drop is logged with its reason, so the timeline shows
/// what was forgotten and why.
#[tauri::command]
fn maintain_insights(state: tauri::State<'_, AppState>) -> Result<forgetting::ForgetPlan, String> {
    let now = now_unix();
    let plan = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let pool = mem.recent_insights(1000).map_err(|e| e.to_string())?;
        let candidates: Vec<forgetting::Candidate> = pool
            .iter()
            .map(|i| forgetting::Candidate {
                id: i.id,
                kind: i.kind.clone(),
                content: i.content.clone(),
                created_at: i.created_at,
                corroborations: i.corroborations,
                uses: i.uses,
                // Read from disk, never computed here: maintenance must not make
                // a network call, and a missing vector just falls back to words.
                embedding: mem.insight_embedding(i.id).ok().flatten(),
            })
            .collect();
        let plan = forgetting::plan(&candidates, now, forgetting::CAPACITY);

        // Credit the keeper before dropping its duplicate, so the evidence
        // survives the copy that carried it.
        for merge in &plan.merges {
            mem.corroborate_insight(merge.keep_id)
                .map_err(|e| e.to_string())?;
        }
        for id in &plan.forget {
            mem.forget_insight(*id).map_err(|e| e.to_string())?;
        }
        plan
    };

    if !plan.forget.is_empty() {
        log_event(
            &state,
            "memory.forgot_insights",
            serde_json::json!({
                "forgot": plan.forget.len(),
                "merged": plan.merges.len(),
                "kept": plan.kept,
                "reasons": plan.reasons,
                "insight_ids": plan.forget.clone(),
            }),
        );
    }
    Ok(plan)
}

// --- Replay v2: step-through player + state audit (§5.4) ---

/// Every frame of the session player, oldest first.
#[tauri::command]
fn replay_timeline(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::core::replay::Step>, String> {
    let events = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2).map_err(|e| e.to_string())?
    };
    Ok(crate::core::replay::timeline(&events))
}

/// The reconstructed world after `steps` events — what the player shows when
/// you scrub to that point.
#[tauri::command]
fn replay_state_at(
    state: tauri::State<'_, AppState>,
    steps: usize,
) -> Result<crate::core::replay::ReplayState, String> {
    let events = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2).map_err(|e| e.to_string())?
    };
    Ok(crate::core::replay::state_at(&events, steps))
}

/// Builds the live world from the real database/filesystem, for comparison.
fn actual_state(state: &AppState) -> Result<crate::core::replay::ReplayState, String> {
    use std::collections::BTreeMap;
    let (messages, insights) = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let messages = mem
            .recent_messages(usize::MAX / 2)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|m| crate::core::replay::ReplayedMessage {
                role: m.role,
                content: m.content,
            })
            .collect();
        let insights = mem
            .recent_insights(usize::MAX / 2)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|i| i.id)
            .collect();
        (messages, insights)
    };
    let mut notes = BTreeMap::new();
    for slug in state.notes.list_notes().map_err(|e| e.to_string())? {
        // Size from the real file, so it can be compared with the log's count.
        let chars = state
            .notes
            .read_note(&slug)
            .map(|c| c.chars().count() as u64)
            .unwrap_or(0);
        notes.insert(slug, chars);
    }
    let skills = state
        .skills
        .list_skills()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|m| (m.name, m.version as u64))
        .collect();
    Ok(crate::core::replay::ReplayState {
        messages,
        notes,
        skills,
        insights,
    })
}

/// Replay v2 audit: reconcile chat, notes and skills at once.
#[tauri::command]
fn replay_audit_state(
    state: tauri::State<'_, AppState>,
) -> Result<crate::core::replay::StateReport, String> {
    let events = {
        let log = state.events.lock().map_err(|e| e.to_string())?;
        log.tail(usize::MAX / 2).map_err(|e| e.to_string())?
    };
    let replayed = crate::core::replay::state_at(&events, events.len());
    let actual = actual_state(&state)?;
    Ok(crate::core::replay::audit_state(&replayed, &actual))
}

// --- semantic memory: search + backfill (§ memory tier two) ---

#[derive(serde::Serialize)]
struct SearchHit {
    id: i64,
    role: String,
    content: String,
    created_at: i64,
    /// Cosine similarity, 0-1ish; the UI shows it as a percentage.
    score: f32,
}

/// Meaning-based search over everything Jarvis remembers. Local-only.
#[tauri::command]
async fn search_memory(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SearchHit>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let query_vec = state.router.embed(q).await?;
    let mem = state.memory.lock().map_err(|e| e.to_string())?;
    let vectors = mem
        .embeddings_for_model(&state.router.embed_model())
        .map_err(|e| e.to_string())?;
    let hits = embedding::top_k(
        &query_vec,
        &vectors,
        limit.unwrap_or(10) as usize,
        // Lower floor than prompt recall: a human reviews these results, so
        // borderline matches are useful here where they'd be noise in a prompt.
        0.3,
        &[],
    );
    let scores: std::collections::HashMap<i64, f32> =
        hits.iter().map(|h| (h.id, h.score)).collect();
    let ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
    Ok(mem
        .messages_by_ids(&ids)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|m| SearchHit {
            score: scores.get(&m.id).copied().unwrap_or(0.0),
            id: m.id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
        })
        .collect())
}

/// Embeds messages that don't have vectors yet (history from before this
/// feature, or turns that arrived while the embedding model was missing).
/// Batched so one call can't run for minutes; returns (indexed, remaining).
#[tauri::command]
async fn index_memory(state: tauri::State<'_, AppState>) -> Result<(u32, u32), String> {
    const BATCH: usize = 100;
    let todo = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let ids = mem
            .unembedded_message_ids(BATCH)
            .map_err(|e| e.to_string())?;
        mem.messages_by_ids(&ids).map_err(|e| e.to_string())?
    };
    let mut indexed = 0u32;
    for m in &todo {
        // First failure aborts the batch: it's almost always "model not
        // pulled", and failing 100 times with the same message helps nobody.
        let vector = state.router.embed(&m.content).await?;
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.upsert_embedding(m.id, &state.router.embed_model(), &vector)
            .map_err(|e| e.to_string())?;
        indexed += 1;
    }
    // Reflection v2: lessons get indexed by the same pass, because duplicate
    // detection by meaning is worthless on a history that predates it. Kept
    // after the messages so a partial run still improves recall first.
    let lessons: Vec<Insight> = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let ids = mem
            .unembedded_insight_ids(BATCH)
            .map_err(|e| e.to_string())?;
        mem.recent_insights(usize::MAX / 2)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|i| ids.contains(&i.id))
            .collect()
    };
    for lesson in &lessons {
        let vector = state.router.embed(&lesson.content).await?;
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.set_insight_embedding(lesson.id, &state.router.embed_model(), &vector)
            .map_err(|e| e.to_string())?;
        indexed += 1;
    }

    let remaining = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        let messages = mem
            .unembedded_message_ids(usize::MAX / 2)
            .map_err(|e| e.to_string())?
            .len() as u32;
        let lessons = mem
            .unembedded_insight_ids(usize::MAX / 2)
            .map_err(|e| e.to_string())?
            .len() as u32;
        messages + lessons
    };
    if indexed > 0 {
        log_event(
            &state,
            "memory.indexed",
            serde_json::json!({ "indexed": indexed, "remaining": remaining }),
        );
    }
    Ok((indexed, remaining))
}

/// Lessons the app dropped on its own. Shown so forgetting is auditable rather
/// than something that quietly happens to your memory.
#[tauri::command]
fn forgotten_insights(state: tauri::State<'_, AppState>) -> Result<Vec<Insight>, String> {
    let mem = state.memory.lock().map_err(|e| e.to_string())?;
    mem.forgotten_insights(200).map_err(|e| e.to_string())
}

/// Puts a forgotten lesson back. The user overruling the scoring is the point:
/// the app's judgement about what matters is a guess, and theirs isn't.
#[tauri::command]
fn restore_insight(state: tauri::State<'_, AppState>, id: i64) -> Result<bool, String> {
    let restored = {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        mem.restore_insight(id).map_err(|e| e.to_string())?
    };
    if restored {
        log_event(
            &state,
            "memory.insight_restored",
            serde_json::json!({ "insight_id": id }),
        );
    }
    Ok(restored)
}

// --- runtime provider settings (custom models; iOS companion enabler) ---

/// Facts keys for saved provider settings. An empty saved value means "no
/// override" and falls back to env/default.
const CFG_KEYS: [(&str, u8); 6] = [
    ("config.ollama_base_url", 0),
    ("config.ollama_model", 1),
    ("config.groq_api_key", 2),
    ("config.groq_model", 3),
    ("config.openrouter_api_key", 4),
    ("config.openrouter_model", 5),
];

/// Overlays saved settings from the facts table onto an env-seeded config.
fn apply_saved_provider_config(mem: &MemoryStore, cfg: &mut RouterConfig) {
    let read = |key: &str| -> Option<String> {
        mem.get_fact(key)
            .ok()
            .flatten()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    for (key, which) in CFG_KEYS {
        let Some(value) = read(key) else { continue };
        match which {
            0 => cfg.ollama_base_url = value,
            1 => cfg.ollama_model = value,
            2 => cfg.groq_api_key = Some(value),
            3 => cfg.groq_model = value,
            4 => cfg.openrouter_api_key = Some(value),
            5 => cfg.openrouter_model = value,
            _ => {}
        }
    }
}

/// The full current provider configuration, for the settings view. This is a
/// local-first app: the values (including keys) are the user's own, stored on
/// their machine, shown back to them in their own settings UI.
#[derive(serde::Serialize)]
struct ProviderSettings {
    ollama_base_url: String,
    ollama_model: String,
    groq_api_key: String,
    groq_model: String,
    openrouter_api_key: String,
    openrouter_model: String,
}

#[tauri::command]
fn get_provider_settings(state: tauri::State<'_, AppState>) -> ProviderSettings {
    let cfg = state.router.config();
    ProviderSettings {
        ollama_base_url: cfg.ollama_base_url,
        ollama_model: cfg.ollama_model,
        groq_api_key: cfg.groq_api_key.unwrap_or_default(),
        groq_model: cfg.groq_model,
        openrouter_api_key: cfg.openrouter_api_key.unwrap_or_default(),
        openrouter_model: cfg.openrouter_model,
    }
}

/// Saves provider settings and applies them immediately — no restart, no .env.
/// Point the Ollama URL at another machine (e.g. your desktop from a phone) and
/// that machine becomes the brain; that is companion mode.
#[tauri::command]
async fn set_provider_settings(
    state: tauri::State<'_, AppState>,
    ollama_base_url: String,
    ollama_model: String,
    groq_api_key: String,
    groq_model: String,
    openrouter_api_key: String,
    openrouter_model: String,
) -> Result<bool, String> {
    let values = [
        ("config.ollama_base_url", ollama_base_url.trim()),
        ("config.ollama_model", ollama_model.trim()),
        ("config.groq_api_key", groq_api_key.trim()),
        ("config.groq_model", groq_model.trim()),
        ("config.openrouter_api_key", openrouter_api_key.trim()),
        ("config.openrouter_model", openrouter_model.trim()),
    ];
    {
        let mem = state.memory.lock().map_err(|e| e.to_string())?;
        for (key, value) in values {
            mem.set_fact(key, value).map_err(|e| e.to_string())?;
        }
        let mut cfg = RouterConfig::from_env();
        apply_saved_provider_config(&mem, &mut cfg);
        state.router.set_config(cfg);
    }
    // Keys never go into the event log; log that settings changed, not what to.
    log_event(
        &state,
        "settings.providers_changed",
        serde_json::json!({ "fields": values.iter().filter(|(_, v)| !v.is_empty()).count() }),
    );
    // Tell the caller whether the (possibly new) local endpoint is reachable,
    // so the settings view can report companion status honestly.
    Ok(state.router.ollama_reachable().await)
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

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Desktop-only ambient presence: a global summon hotkey and launch-at-login.
    // The tray itself is wired inside setup() so it can read the autostart state.
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_global_shortcut::Builder::new().build())
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ));
    }

    builder
        .setup(|app| {
            let data_dir = resolve_data_dir(app)?;
            let memory = MemoryStore::open(&data_dir.join("jarvis.sqlite3"))?;
            // Providers: env seeds the defaults, then anything saved from the
            // settings view wins. On iOS there are no env vars at all, so the
            // DB overlay is the only way the phone can be configured — this is
            // what makes companion mode (phone -> desktop Ollama) possible.
            let mut router_cfg = RouterConfig::from_env();
            apply_saved_provider_config(&memory, &mut router_cfg);
            let router = Router::new(router_cfg);
            // Read the wake phrase before `memory` moves into AppState.
            #[cfg(desktop)]
            let wake_phrase = saved_wake_phrase(&memory);
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
                data_dir,
                #[cfg(desktop)]
                recorder: crate::mic::MicRecorder::new(),
                #[cfg(feature = "local-whisper")]
                transcriber: Mutex::new(None),
                #[cfg(desktop)]
                session: Mutex::new(crate::core::conversation::Session::new(wake_phrase)),
            });
            #[cfg(desktop)]
            setup_desktop_ambient(app)?;
            // Auto mode's heartbeat. Safe to start unconditionally: it does
            // nothing at all until the gates pass, and auto mode defaults off.
            #[cfg(desktop)]
            spawn_heartbeat(app.handle().clone());
            Ok(())
        })
        // Closing the window hides Jarvis to the tray instead of quitting, so it
        // stays a keystroke away. Quitting for real is the tray's "Quit" item.
        .on_window_event(|window, event| {
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
            #[cfg(not(desktop))]
            {
                let _ = (window, event);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            chat_send,
            get_history,
            get_telemetry,
            get_events,
            save_note,
            delete_note,
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
            wipe_memory,
            rate_message,
            calibration_report,
            maintain_insights,
            get_provider_settings,
            set_provider_settings,
            search_memory,
            index_memory,
            replay_timeline,
            replay_state_at,
            replay_audit_state,
            forgotten_insights,
            restore_insight,
            autonomy_state,
            autonomy_last_beat,
            autonomy_set_enabled,
            autonomy_set_caps,
            autonomy_stop_file,
            autonomy_plan,
            autonomy_run_cycle,
            voice_session,
            voice_watch_barge,
            voice_stop_barge_watch,
            voice_hands_free,
            voice_set_wake_phrase,
            voice_heard,
            voice_advance,
            stt_status,
            stt_device,
            stt_listen,
            stt_download_model,
            stt_start,
            stt_stop,
            stt_cancel
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// --- desktop ambient presence: tray, global hotkey, launch-at-login (§6.5) ---

/// Wire the system tray and the global summon hotkey. Called from setup() on
/// desktop only. The tray gives Jarvis a home when its window is hidden; the
/// hotkey (Ctrl+Shift+J) brings it back from anywhere.
#[cfg(desktop)]
fn setup_desktop_ambient(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri_plugin_autostart::ManagerExt;
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    let handle = app.handle();

    // Tray menu: show, a checkbox that mirrors the OS launch-at-login state, quit.
    let autostart_on = handle.autolaunch().is_enabled().unwrap_or(false);
    let show_item = MenuItemBuilder::with_id("tray_show", "Show H.O.T-Jarvis").build(app)?;
    let autostart_item = CheckMenuItemBuilder::with_id("tray_autostart", "Start at login")
        .checked(autostart_on)
        .build(app)?;
    let quit_item = MenuItemBuilder::with_id("tray_quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &autostart_item, &quit_item])
        .build()?;

    TrayIconBuilder::with_id("main")
        .tooltip("H.O.T-Jarvis")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or("no default window icon")?,
        )
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_main_window(app),
            "tray_quit" => app.exit(0),
            "tray_autostart" => {
                let mgr = app.autolaunch();
                if mgr.is_enabled().unwrap_or(false) {
                    let _ = mgr.disable();
                } else {
                    let _ = mgr.enable();
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // A left click on the icon toggles the window, like most tray apps.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    // Global summon: Ctrl+Shift+J toggles the window from anywhere.
    let toggle = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyJ);
    handle
        .global_shortcut()
        .on_shortcut(toggle, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_main_window(app);
            }
        })?;

    Ok(())
}

#[cfg(desktop)]
fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(desktop)]
fn toggle_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}
