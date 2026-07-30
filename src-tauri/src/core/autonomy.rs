//! Auto mode guardrails (§7, and the mandatory rules in §2).
//!
//! The roadmap blocked autonomy on "needs CI + guardrails proven". This module
//! is those guardrails, as code with tests, rather than as prose that a loop
//! might or might not honour.
//!
//! The safety model is a deny-by-default allowlist, because that is the only
//! shape that fails safe. A new action added anywhere in the app is `Forbidden`
//! for the autonomous loop until someone deliberately classifies it — so
//! forgetting to think about safety produces a refusal, not an incident.
//!
//! Three independent things must all hold before a cycle runs:
//!   1. The kill switch is off (a file flag *or* an env var — either stops it).
//!   2. The budget has room (tasks, tool calls, wall clock, and a cycle gap).
//!   3. The action itself is allowlisted, and not merely "probably fine".
//!
//! Everything here is pure: no filesystem, no clock, no model. Time and the
//! kill-switch file are passed in, which is what makes the policy testable.

use serde::{Deserialize, Serialize};

// --- 1. What the autonomous loop is allowed to do at all ---

/// Every action the loop could conceivably take. Exhaustive on purpose: adding
/// a variant forces a decision in `classify`, and the compiler enforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Distil lessons from the event log. Additive, and undoable since v2.
    Reflect,
    /// Merge duplicate lessons and drop spent ones. Logged with reasons.
    TidyInsights,
    /// Backfill embeddings for old messages. Purely additive.
    IndexMemory,
    /// Re-run existing skill tests. Read-only verification.
    TestSkills,
    /// Check the log still reproduces reality. Read-only.
    ReplayAudit,

    /// Write a note. Reversible, but it creates user-visible content.
    SaveNote,
    /// Have the model write a new skill. Code generation.
    AuthorSkill,
    /// Execute a skill. Code execution, even if sandboxed.
    RunSkill,

    /// Erase memory. Irreversible by design.
    WipeMemory,
    /// Delete a note.
    DeleteNote,
    /// Change providers or keys.
    ChangeSettings,
    /// Anything touching the network, the filesystem outside the sandbox, or
    /// another person.
    ExternalSideEffect,
}

/// How the loop may treat an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Clearance {
    /// Safe to do unattended: bounded, sandboxed, and reversible or read-only.
    Auto,
    /// May be *planned* and shown, never performed without a human yes (§2).
    NeedsApproval,
    /// Never, regardless of approval prompts inside the loop. A human does this
    /// deliberately in the UI, not via automation.
    Forbidden,
}

/// The §2 guardrails, as a total function.
///
/// The line between `Auto` and `NeedsApproval` is drawn at *creating or running
/// content*, not at "is it reversible". Authoring and running skills is the
/// project's hero feature and it is sandboxed and undoable — but a loop that
/// writes and executes code with nobody asking is a different thing from a user
/// requesting a skill, so it stays behind approval.
pub fn classify(action: ActionKind) -> Clearance {
    match action {
        // Self-maintenance: bounded, sandboxed, reversible or read-only.
        ActionKind::Reflect
        | ActionKind::TidyInsights
        | ActionKind::IndexMemory
        | ActionKind::TestSkills
        | ActionKind::ReplayAudit => Clearance::Auto,

        // Creates content or runs code. Plan it, show it, wait.
        ActionKind::SaveNote | ActionKind::AuthorSkill | ActionKind::RunSkill => {
            Clearance::NeedsApproval
        }

        // Irreversible, or reaches outside the sandbox.
        ActionKind::WipeMemory
        | ActionKind::DeleteNote
        | ActionKind::ChangeSettings
        | ActionKind::ExternalSideEffect => Clearance::Forbidden,
    }
}

/// The actions a cycle may perform unattended, in the order it prefers them.
/// Cheap and verifying first, so a cycle that runs out of budget has still done
/// something useful rather than half a reflection.
pub fn auto_actions() -> Vec<ActionKind> {
    vec![
        ActionKind::ReplayAudit,
        ActionKind::TestSkills,
        ActionKind::IndexMemory,
        ActionKind::Reflect,
        ActionKind::TidyInsights,
    ]
}

// --- 2. Resource caps ---

/// Per-cycle limits. Defaults are deliberately conservative: the brief says so,
/// and an autonomous loop that surprises you is worse than one that does less.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caps {
    /// Distinct actions per cycle.
    pub max_actions: u32,
    /// Model/tool calls per cycle — the thing that actually costs time and rate
    /// limit on a free tier.
    pub max_tool_calls: u32,
    /// Wall-clock seconds for the whole cycle.
    pub max_seconds: u32,
    /// Minimum gap between cycles, so a heartbeat can't become a busy loop.
    pub min_cycle_gap_secs: i64,
    /// How long the user must have been quiet before an unattended cycle runs.
    ///
    /// This is the difference between a background loop that is helpful and one
    /// that is rude: reflection and indexing both make model calls, and doing
    /// that while someone is mid-conversation makes the app feel slow for no
    /// visible reason. Deferring costs nothing — the work is never urgent.
    #[serde(default = "default_min_idle_secs")]
    pub min_idle_secs: i64,
}

fn default_min_idle_secs() -> i64 {
    120
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            max_actions: 3,
            max_tool_calls: 6,
            max_seconds: 120,
            min_cycle_gap_secs: 900, // 15 minutes
            min_idle_secs: default_min_idle_secs(),
        }
    }
}

/// Usage accumulated during one cycle. Logged, per §2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Usage {
    pub actions: u32,
    pub tool_calls: u32,
    pub seconds: u32,
}

/// Why a cycle stopped. Reported rather than inferred, so the log says what
/// actually happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Everything the cycle wanted to do is done.
    Completed,
    ActionCap,
    ToolCallCap,
    TimeCap,
    /// The kill switch went on mid-cycle.
    Killed,
}

impl Usage {
    /// Does this usage still leave room for one more action costing
    /// `tool_calls`? Checked *before* acting, so a cap is never exceeded rather
    /// than merely detected afterwards.
    pub fn room_for(&self, caps: &Caps, tool_calls: u32) -> Option<StopReason> {
        if self.actions >= caps.max_actions {
            return Some(StopReason::ActionCap);
        }
        if self.tool_calls + tool_calls > caps.max_tool_calls {
            return Some(StopReason::ToolCallCap);
        }
        if self.seconds >= caps.max_seconds {
            return Some(StopReason::TimeCap);
        }
        None
    }

    pub fn record(&mut self, tool_calls: u32, seconds: u32) {
        self.actions += 1;
        self.tool_calls += tool_calls;
        self.seconds += seconds;
    }
}

// --- 3. The kill switch ---

/// Why the loop is halted, if it is. Two independent mechanisms because the
/// brief asks for both, and because a file the user can `touch` is the one that
/// works when the UI is unresponsive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum Halt {
    /// `.jarvis/STOP` exists.
    StopFile,
    /// `JARVIS_AUTONOMY=off` (or any value that isn't clearly "on").
    EnvVar,
    /// Auto mode was never switched on.
    Disabled,
    /// Cycles are rate-limited; this one is too soon.
    TooSoon { wait_secs: i64 },
    /// The user is actively using the app; unattended work waits.
    Busy { wait_secs: i64 },
}

/// Reads the env var into a decision. Anything other than an explicit on-ish
/// value halts: an autonomous loop should not start because a variable was
/// misspelled.
pub fn env_allows(value: Option<&str>) -> bool {
    match value.map(|v| v.trim().to_lowercase()) {
        None => true, // unset means "no opinion", the toggle decides
        Some(v) => matches!(v.as_str(), "on" | "1" | "true" | "yes" | "enabled"),
    }
}

/// The single gate a cycle must pass. `stop_file_exists` and `now` are passed in
/// so this is pure and testable.
pub fn may_start(
    enabled: bool,
    stop_file_exists: bool,
    env: Option<&str>,
    last_cycle_at: Option<i64>,
    last_user_activity: Option<i64>,
    now: i64,
    caps: &Caps,
) -> Result<(), Halt> {
    // The file flag wins over everything, including "enabled": it is the
    // emergency brake, and an emergency brake that can be overridden is not one.
    if stop_file_exists {
        return Err(Halt::StopFile);
    }
    if !env_allows(env) {
        return Err(Halt::EnvVar);
    }
    if !enabled {
        return Err(Halt::Disabled);
    }
    if let Some(last) = last_cycle_at {
        let elapsed = now.saturating_sub(last);
        if elapsed < caps.min_cycle_gap_secs {
            return Err(Halt::TooSoon {
                wait_secs: caps.min_cycle_gap_secs - elapsed,
            });
        }
    }
    // Softest gate, checked last so a hard halt is reported in preference to a
    // temporary one.
    if let Some(active) = last_user_activity {
        let quiet = now.saturating_sub(active);
        if quiet < caps.min_idle_secs {
            return Err(Halt::Busy {
                wait_secs: caps.min_idle_secs - quiet,
            });
        }
    }
    Ok(())
}

// --- 5. The heartbeat ---

/// How often the background loop should wake to *check* whether a cycle may run.
///
/// Deliberately not the cycle interval: waking cheaply and often keeps the STOP
/// file responsive, while `may_start` is what decides whether anything actually
/// happens. Bounded to 5..=60s so it is neither a busy loop nor so sleepy that
/// disarming appears not to work.
pub fn heartbeat_poll_secs(caps: &Caps) -> u64 {
    let quarter = (caps.min_cycle_gap_secs / 4).max(1) as u64;
    quarter.clamp(5, 60)
}

/// What the heartbeat did on one wake-up, for the UI and the log. A heartbeat
/// that runs invisibly is indistinguishable from one that is broken.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum Beat {
    /// A gate refused; nothing happened.
    Held { halt: Halt },
    /// Gates passed but the planner had nothing worth doing.
    Idle,
    /// A cycle ran.
    Ran { actions: u32 },
}

// --- 4. The plan ---

/// One thing a cycle intends to do.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedAction {
    pub action: ActionKind,
    pub clearance: Clearance,
    /// Estimated tool calls, used for budgeting before acting.
    pub tool_calls: u32,
    /// Why the loop wants to do this, in plain words, for the log and the UI.
    pub reason: String,
}

/// What a cycle would do. Produced first and shown; executing it is a separate,
/// explicit step — that is the dry-run gate from §7.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CyclePlan {
    pub actions: Vec<PlannedAction>,
    /// Actions that were wanted but need a human yes.
    pub deferred: Vec<PlannedAction>,
    pub stop_reason: StopReason,
    pub caps: Caps,
    /// True when nothing at all is worth doing right now.
    pub idle: bool,
}

/// What the app currently looks like, as far as the planner needs to know.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AppSnapshot {
    /// Messages with no embedding yet.
    pub unindexed_messages: u32,
    /// Events since the last reflection pass.
    pub events_since_reflection: u32,
    /// Lessons currently stored.
    pub insights: u32,
    /// Skills whose tests haven't been run this session.
    pub untested_skills: u32,
}

/// Estimated tool calls per action, so budgeting happens before spending.
fn cost(action: ActionKind) -> u32 {
    match action {
        // One model call each.
        ActionKind::Reflect => 1,
        // A call per message, so it is capped hard by the planner instead.
        ActionKind::IndexMemory => 2,
        // Local only.
        ActionKind::ReplayAudit | ActionKind::TidyInsights | ActionKind::TestSkills => 0,
        _ => 1,
    }
}

/// Is this action worth doing given the current state? Keeps a cycle from
/// burning budget on no-ops, which is what makes a 15-minute heartbeat cheap.
fn worthwhile(action: ActionKind, snap: &AppSnapshot) -> Option<String> {
    match action {
        ActionKind::ReplayAudit => Some("verify the log still reproduces reality".into()),
        ActionKind::TestSkills if snap.untested_skills > 0 => Some(format!(
            "{} skill(s) haven't been verified this session",
            snap.untested_skills
        )),
        ActionKind::IndexMemory if snap.unindexed_messages > 0 => Some(format!(
            "{} message(s) aren't searchable yet",
            snap.unindexed_messages
        )),
        ActionKind::Reflect if snap.events_since_reflection >= 20 => Some(format!(
            "{} events since the last reflection",
            snap.events_since_reflection
        )),
        ActionKind::TidyInsights if snap.insights >= 25 => Some(format!(
            "{} lessons stored; worth merging duplicates and dropping spent ones",
            snap.insights
        )),
        _ => None,
    }
}

/// Builds the plan for one cycle. Never performs anything.
///
/// Deny-by-default in practice as well as in principle: only actions from
/// `auto_actions()` are even considered, and each is re-checked against
/// `classify` — so an allowlist edit alone can't smuggle something through.
pub fn plan_cycle(snap: &AppSnapshot, caps: &Caps) -> CyclePlan {
    let mut actions = Vec::new();
    let mut deferred = Vec::new();
    let mut usage = Usage::default();
    let mut stop_reason = StopReason::Completed;

    for action in auto_actions() {
        let Some(reason) = worthwhile(action, snap) else {
            continue;
        };
        let clearance = classify(action);
        let planned = PlannedAction {
            action,
            clearance,
            tool_calls: cost(action),
            reason,
        };
        match clearance {
            // Belt and braces: even inside auto_actions, anything not cleared
            // for unattended work is deferred rather than run.
            Clearance::NeedsApproval => {
                deferred.push(planned);
                continue;
            }
            Clearance::Forbidden => continue,
            Clearance::Auto => {}
        }
        if let Some(stop) = usage.room_for(caps, planned.tool_calls) {
            stop_reason = stop;
            break;
        }
        usage.record(planned.tool_calls, 0);
        actions.push(planned);
    }

    CyclePlan {
        idle: actions.is_empty() && deferred.is_empty(),
        actions,
        deferred,
        stop_reason,
        caps: caps.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn busy() -> AppSnapshot {
        AppSnapshot {
            unindexed_messages: 40,
            events_since_reflection: 50,
            insights: 60,
            untested_skills: 2,
        }
    }

    // --- the allowlist ---

    #[test]
    fn destructive_and_external_actions_are_forbidden_outright() {
        for action in [
            ActionKind::WipeMemory,
            ActionKind::DeleteNote,
            ActionKind::ChangeSettings,
            ActionKind::ExternalSideEffect,
        ] {
            assert_eq!(
                classify(action),
                Clearance::Forbidden,
                "{action:?} must never run unattended"
            );
        }
    }

    #[test]
    fn creating_content_or_running_code_needs_a_human() {
        // Sandboxed and undoable is not the same as "fine to do unasked".
        for action in [
            ActionKind::SaveNote,
            ActionKind::AuthorSkill,
            ActionKind::RunSkill,
        ] {
            assert_eq!(classify(action), Clearance::NeedsApproval, "{action:?}");
        }
    }

    #[test]
    fn only_self_maintenance_is_cleared_for_unattended_work() {
        for action in [
            ActionKind::Reflect,
            ActionKind::TidyInsights,
            ActionKind::IndexMemory,
            ActionKind::TestSkills,
            ActionKind::ReplayAudit,
        ] {
            assert_eq!(classify(action), Clearance::Auto, "{action:?}");
        }
    }

    #[test]
    fn every_auto_action_is_actually_cleared_for_auto() {
        // Guards against the allowlist and the classifier drifting apart — the
        // exact bug that would quietly widen what the loop can do.
        for action in auto_actions() {
            assert_eq!(
                classify(action),
                Clearance::Auto,
                "{action:?} is in auto_actions but not cleared"
            );
        }
    }

    #[test]
    fn the_auto_list_holds_no_forbidden_or_approval_actions() {
        let list = auto_actions();
        for action in [
            ActionKind::WipeMemory,
            ActionKind::AuthorSkill,
            ActionKind::RunSkill,
            ActionKind::DeleteNote,
            ActionKind::SaveNote,
            ActionKind::ChangeSettings,
            ActionKind::ExternalSideEffect,
        ] {
            assert!(!list.contains(&action), "{action:?} must not be automatic");
        }
    }

    // --- the kill switch ---

    #[test]
    fn the_stop_file_overrides_everything_including_enabled() {
        // An emergency brake that can be overridden is not an emergency brake.
        assert_eq!(
            may_start(true, true, Some("on"), None, None, NOW, &Caps::default()),
            Err(Halt::StopFile)
        );
    }

    #[test]
    fn the_env_var_halts_unless_it_clearly_says_on() {
        let caps = Caps::default();
        for value in ["off", "0", "false", "no", "", "maybe", "ON!"] {
            assert_eq!(
                may_start(true, false, Some(value), None, None, NOW, &caps),
                Err(Halt::EnvVar),
                "{value:?} must not enable autonomy"
            );
        }
        for value in ["on", "1", "true", "yes", "enabled", " ON "] {
            assert!(
                may_start(true, false, Some(value), None, None, NOW, &caps).is_ok(),
                "{value:?} should be accepted"
            );
        }
        // Unset means no opinion; the in-app toggle decides.
        assert!(may_start(true, false, None, None, None, NOW, &caps).is_ok());
        assert_eq!(
            may_start(false, false, None, None, None, NOW, &caps),
            Err(Halt::Disabled)
        );
    }

    #[test]
    fn cycles_are_rate_limited_so_a_heartbeat_cannot_busy_loop() {
        let caps = Caps::default();
        // Just ran.
        assert_eq!(
            may_start(true, false, None, Some(NOW - 10), None, NOW, &caps),
            Err(Halt::TooSoon {
                wait_secs: caps.min_cycle_gap_secs - 10
            })
        );
        // Long enough ago.
        assert!(may_start(
            true,
            false,
            None,
            Some(NOW - caps.min_cycle_gap_secs),
            None,
            NOW,
            &caps
        )
        .is_ok());
        // A clock that jumped backwards must not unlock the gate.
        assert!(may_start(true, false, None, Some(NOW + 5_000), None, NOW, &caps).is_err());
    }

    // --- the idle gate ---

    #[test]
    fn an_unattended_cycle_waits_while_the_user_is_active() {
        // Reflection and indexing both make model calls; doing that mid-
        // conversation makes the app feel slow for no visible reason.
        let caps = Caps::default();
        assert_eq!(
            may_start(true, false, None, None, Some(NOW - 5), NOW, &caps),
            Err(Halt::Busy {
                wait_secs: caps.min_idle_secs - 5
            })
        );
    }

    #[test]
    fn a_quiet_app_is_allowed_to_work() {
        let caps = Caps::default();
        assert!(may_start(
            true,
            false,
            None,
            None,
            Some(NOW - caps.min_idle_secs),
            NOW,
            &caps
        )
        .is_ok());
        // Never used at all counts as idle, not as blocked.
        assert!(may_start(true, false, None, None, None, NOW, &caps).is_ok());
    }

    #[test]
    fn hard_halts_are_reported_in_preference_to_being_busy() {
        // If the STOP file is set *and* the user is active, the useful answer is
        // "you stopped it", not "wait 2 minutes".
        let caps = Caps::default();
        assert_eq!(
            may_start(true, true, None, None, Some(NOW), NOW, &caps),
            Err(Halt::StopFile)
        );
        assert_eq!(
            may_start(false, false, None, None, Some(NOW), NOW, &caps),
            Err(Halt::Disabled)
        );
        assert_eq!(
            may_start(true, false, Some("off"), None, Some(NOW), NOW, &caps),
            Err(Halt::EnvVar)
        );
    }

    #[test]
    fn a_backwards_clock_does_not_unlock_the_idle_gate() {
        let caps = Caps::default();
        assert!(may_start(true, false, None, None, Some(NOW + 5_000), NOW, &caps).is_err());
    }

    // --- the heartbeat ---

    #[test]
    fn the_poll_interval_is_bounded_at_both_ends() {
        // Never a busy loop, never so sleepy that disarming looks broken.
        for gap in [0, 1, 60, 900, 86_400] {
            let caps = Caps {
                min_cycle_gap_secs: gap,
                ..Caps::default()
            };
            let poll = heartbeat_poll_secs(&caps);
            assert!((5..=60).contains(&poll), "gap {gap} gave poll {poll}");
        }
    }

    #[test]
    fn the_poll_interval_is_shorter_than_the_cycle_gap() {
        // Otherwise a cycle could be skipped entirely by unlucky timing.
        let caps = Caps::default();
        let poll = heartbeat_poll_secs(&caps) as i64;
        assert!(poll < caps.min_cycle_gap_secs, "poll {poll}");
    }

    #[test]
    fn a_beat_reports_what_actually_happened() {
        // A heartbeat that runs invisibly is indistinguishable from a broken one.
        let held = serde_json::to_string(&Beat::Held {
            halt: Halt::Busy { wait_secs: 30 },
        })
        .unwrap();
        assert!(held.contains("\"outcome\":\"held\""), "{held}");
        assert!(held.contains("busy"), "{held}");
        assert!(serde_json::to_string(&Beat::Idle).unwrap().contains("idle"));
        assert!(serde_json::to_string(&Beat::Ran { actions: 2 })
            .unwrap()
            .contains("\"actions\":2"));
    }

    #[test]
    fn caps_from_an_older_config_still_load() {
        // min_idle_secs was added after the first release; a stored config
        // without it must not fail to parse and disable auto mode silently.
        let legacy =
            r#"{"max_actions":3,"max_tool_calls":6,"max_seconds":120,"min_cycle_gap_secs":900}"#;
        let caps: Caps = serde_json::from_str(legacy).unwrap();
        assert_eq!(caps.min_idle_secs, 120, "falls back to the default");
    }

    // --- caps ---

    #[test]
    fn defaults_are_conservative() {
        let caps = Caps::default();
        assert!(caps.max_actions <= 5);
        assert!(caps.max_tool_calls <= 10);
        assert!(caps.max_seconds <= 300);
        assert!(caps.min_cycle_gap_secs >= 300);
    }

    #[test]
    fn room_for_checks_before_spending_not_after() {
        let caps = Caps {
            max_actions: 2,
            max_tool_calls: 3,
            max_seconds: 60,
            min_cycle_gap_secs: 0,
            min_idle_secs: 0,
        };
        let mut usage = Usage::default();
        assert_eq!(usage.room_for(&caps, 1), None);
        usage.record(1, 10);
        assert_eq!(usage.room_for(&caps, 1), None);
        usage.record(1, 10);
        // Action cap reached.
        assert_eq!(usage.room_for(&caps, 0), Some(StopReason::ActionCap));

        // A call that *would* exceed the tool budget is refused up front.
        let mut usage = Usage::default();
        usage.record(2, 0);
        assert_eq!(usage.room_for(&caps, 2), Some(StopReason::ToolCallCap));
        assert_eq!(usage.room_for(&caps, 1), None, "exactly at the cap is fine");
    }

    #[test]
    fn the_time_cap_stops_a_cycle() {
        let caps = Caps {
            max_seconds: 30,
            ..Caps::default()
        };
        let mut usage = Usage::default();
        usage.record(0, 30);
        assert_eq!(usage.room_for(&caps, 0), Some(StopReason::TimeCap));
    }

    // --- planning ---

    #[test]
    fn a_quiet_app_plans_almost_nothing() {
        // Nothing to index, nothing to reflect on, few lessons: the audit is the
        // only thing worth doing, and it costs no model calls.
        let plan = plan_cycle(&AppSnapshot::default(), &Caps::default());
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].action, ActionKind::ReplayAudit);
        assert_eq!(plan.actions[0].tool_calls, 0);
        assert!(!plan.idle);
    }

    #[test]
    fn a_busy_app_plans_cheap_verification_before_expensive_work() {
        let plan = plan_cycle(&busy(), &Caps::default());
        assert_eq!(
            plan.actions[0].action,
            ActionKind::ReplayAudit,
            "free checks first, so a truncated cycle still did something"
        );
        assert!(plan.actions.iter().all(|a| a.clearance == Clearance::Auto));
        // Default cap is 3 actions.
        assert_eq!(plan.actions.len(), 3);
        assert_eq!(plan.stop_reason, StopReason::ActionCap);
    }

    #[test]
    fn planning_never_includes_an_action_needing_approval() {
        let plan = plan_cycle(&busy(), &Caps::default());
        for planned in &plan.actions {
            assert_ne!(planned.clearance, Clearance::NeedsApproval);
            assert_ne!(planned.clearance, Clearance::Forbidden);
        }
    }

    #[test]
    fn the_plan_respects_the_tool_call_budget() {
        let caps = Caps {
            max_actions: 10,
            max_tool_calls: 2,
            max_seconds: 600,
            min_cycle_gap_secs: 0,
            min_idle_secs: 0,
        };
        let plan = plan_cycle(&busy(), &caps);
        let spent: u32 = plan.actions.iter().map(|a| a.tool_calls).sum();
        assert!(spent <= caps.max_tool_calls, "spent {spent}");
        assert_eq!(plan.stop_reason, StopReason::ToolCallCap);
    }

    #[test]
    fn reflection_waits_until_there_is_something_to_reflect_on() {
        let nearly = AppSnapshot {
            events_since_reflection: 19,
            ..AppSnapshot::default()
        };
        let plan = plan_cycle(&nearly, &Caps::default());
        assert!(!plan.actions.iter().any(|a| a.action == ActionKind::Reflect));

        let ready = AppSnapshot {
            events_since_reflection: 20,
            ..AppSnapshot::default()
        };
        let plan = plan_cycle(&ready, &Caps::default());
        assert!(plan.actions.iter().any(|a| a.action == ActionKind::Reflect));
    }

    #[test]
    fn every_planned_action_explains_itself() {
        // The log and the UI both show this; an unexplained autonomous action is
        // exactly what makes auto mode feel untrustworthy.
        let plan = plan_cycle(&busy(), &Caps::default());
        for planned in &plan.actions {
            assert!(
                planned.reason.len() > 10,
                "{:?} has no real reason: {:?}",
                planned.action,
                planned.reason
            );
        }
    }

    #[test]
    fn planning_is_deterministic() {
        let snap = busy();
        let caps = Caps::default();
        assert_eq!(plan_cycle(&snap, &caps), plan_cycle(&snap, &caps));
    }

    #[test]
    fn the_plan_serializes_for_the_ui() {
        let json = serde_json::to_string(&plan_cycle(&busy(), &Caps::default())).unwrap();
        for key in ["actions", "deferred", "stop_reason", "caps", "idle"] {
            assert!(json.contains(key), "missing {key}");
        }
        assert!(json.contains("\"clearance\":\"auto\""));
    }

    #[test]
    fn caps_round_trip_through_config() {
        let caps = Caps {
            max_actions: 7,
            max_tool_calls: 9,
            max_seconds: 45,
            min_cycle_gap_secs: 600,
            min_idle_secs: 90,
        };
        let json = serde_json::to_string(&caps).unwrap();
        assert_eq!(serde_json::from_str::<Caps>(&json).unwrap(), caps);
    }
}
