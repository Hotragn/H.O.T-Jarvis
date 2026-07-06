//! Skill engine v0 (§5.1): skills are self-contained, discoverable units —
//! a manifest, Rhai code defining `fn run(input)`, and a bundled test
//! defining `fn test()`. Every save runs the test; a skill whose test fails
//! is flagged for refinement and refused at run time, never used blindly.
//! Rhai gives a sandbox by construction: scripts see only the language
//! built-ins (no filesystem, no network) and run under an operation cap so
//! a runaway loop terminates instead of hanging the assistant.
//!
//! Layout on disk, one directory per skill under `<data_dir>/skills/`:
//!   <slug>/manifest.json     name, version, description, test status
//!   <slug>/skill.rhai        the code
//!   <slug>/test.rhai         the bundled test
//!   <slug>/history/v<N>/     archived previous versions

use crate::core::tools::slugify;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_OPERATIONS: u64 = 200_000;
const MAX_CALL_LEVELS: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("skill name must contain at least one letter or digit")]
    InvalidName,
    #[error("skill not found: {0}")]
    NotFound(String),
    #[error("skill '{0}' is flagged: its test failed ({1}) — refine it before use")]
    Flagged(String, String),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest error: {0}")]
    Manifest(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "detail", rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: u32,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub test_status: TestStatus,
}

pub struct SkillEngine {
    skills_dir: PathBuf,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// One sandboxed interpreter per execution: fresh state, hard caps.
fn sandboxed_engine() -> rhai::Engine {
    let mut engine = rhai::Engine::new();
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_call_levels(MAX_CALL_LEVELS);
    engine.set_max_expr_depths(64, 64);
    engine.set_max_string_size(64 * 1024);
    engine.set_max_array_size(4 * 1024);
    engine.set_max_map_size(1024);
    engine
}

/// Runs the bundled test against the skill code. The test sees the skill's
/// functions (sources are compiled together) and must define `fn test()`
/// returning `true` for a pass.
pub fn run_test(code: &str, test: &str) -> TestStatus {
    let engine = sandboxed_engine();
    let combined = format!("{code}\n\n{test}");
    let ast = match engine.compile(&combined) {
        Ok(ast) => ast,
        Err(e) => return TestStatus::Failed(format!("compile error: {e}")),
    };
    let mut scope = rhai::Scope::new();
    match engine.call_fn::<bool>(&mut scope, &ast, "test", ()) {
        Ok(true) => TestStatus::Passed,
        Ok(false) => TestStatus::Failed("test() returned false".into()),
        Err(e) => TestStatus::Failed(e.to_string()),
    }
}

impl SkillEngine {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            skills_dir: data_dir.join("skills"),
        }
    }

    fn dir_of(&self, slug: &str) -> PathBuf {
        self.skills_dir.join(slug)
    }

    fn read_manifest(&self, slug: &str) -> Result<SkillManifest, SkillError> {
        let path = self.dir_of(slug).join("manifest.json");
        if !path.exists() {
            return Err(SkillError::NotFound(slug.to_string()));
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    fn write_manifest(&self, slug: &str, manifest: &SkillManifest) -> Result<(), SkillError> {
        fs::write(
            self.dir_of(slug).join("manifest.json"),
            serde_json::to_string_pretty(manifest)?,
        )?;
        Ok(())
    }

    /// Saves (or updates) a skill and immediately runs its bundled test.
    /// Updates bump the version and archive the previous sources.
    pub fn save_skill(
        &self,
        name: &str,
        description: &str,
        code: &str,
        test: &str,
    ) -> Result<SkillManifest, SkillError> {
        let slug = slugify(name).ok_or(SkillError::InvalidName)?;
        let dir = self.dir_of(&slug);
        let now = now_unix();

        let (version, created_at) = match self.read_manifest(&slug) {
            Ok(previous) => {
                // Archive the outgoing version before overwriting.
                let archive = dir.join("history").join(format!("v{}", previous.version));
                fs::create_dir_all(&archive)?;
                for file in ["skill.rhai", "test.rhai"] {
                    let src = dir.join(file);
                    if src.exists() {
                        fs::copy(&src, archive.join(file))?;
                    }
                }
                (previous.version + 1, previous.created_at)
            }
            Err(_) => (1, now),
        };

        fs::create_dir_all(&dir)?;
        fs::write(dir.join("skill.rhai"), code)?;
        fs::write(dir.join("test.rhai"), test)?;

        let manifest = SkillManifest {
            name: slug.clone(),
            version,
            description: description.trim().to_string(),
            created_at,
            updated_at: now,
            test_status: run_test(code, test),
        };
        self.write_manifest(&slug, &manifest)?;
        Ok(manifest)
    }

    pub fn list_skills(&self) -> Result<Vec<SkillManifest>, SkillError> {
        if !self.skills_dir.exists() {
            return Ok(Vec::new());
        }
        let mut skills: Vec<SkillManifest> = fs::read_dir(&self.skills_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                self.read_manifest(&entry.file_name().to_string_lossy())
                    .ok()
            })
            .collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }

    /// Re-runs the bundled test and persists the outcome.
    pub fn test_skill(&self, name: &str) -> Result<SkillManifest, SkillError> {
        let slug = slugify(name).ok_or(SkillError::InvalidName)?;
        let mut manifest = self.read_manifest(&slug)?;
        let dir = self.dir_of(&slug);
        let code = fs::read_to_string(dir.join("skill.rhai"))?;
        let test = fs::read_to_string(dir.join("test.rhai"))?;
        manifest.test_status = run_test(&code, &test);
        manifest.updated_at = now_unix();
        self.write_manifest(&slug, &manifest)?;
        Ok(manifest)
    }

    /// Executes `fn run(input)`. A skill whose last test failed is refused —
    /// flagged for refinement, not used blindly (§5.1).
    pub fn run_skill(&self, name: &str, input: &str) -> Result<String, SkillError> {
        let slug = slugify(name).ok_or(SkillError::InvalidName)?;
        let manifest = self.read_manifest(&slug)?;
        if let TestStatus::Failed(detail) = &manifest.test_status {
            return Err(SkillError::Flagged(slug, detail.clone()));
        }
        let code = fs::read_to_string(self.dir_of(&slug).join("skill.rhai"))?;
        let engine = sandboxed_engine();
        let ast = engine
            .compile(&code)
            .map_err(|e| SkillError::Execution(format!("compile error: {e}")))?;
        let mut scope = rhai::Scope::new();
        let result: rhai::Dynamic = engine
            .call_fn(&mut scope, &ast, "run", (input.to_string(),))
            .map_err(|e| SkillError::Execution(e.to_string()))?;
        Ok(result.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREET_CODE: &str = r#"fn run(input) { "You said: " + input }"#;
    const GREET_TEST: &str = r#"fn test() { run("hi") == "You said: hi" }"#;

    fn engine() -> (tempfile::TempDir, SkillEngine) {
        let dir = tempfile::tempdir().unwrap();
        let engine = SkillEngine::new(dir.path());
        (dir, engine)
    }

    #[test]
    fn save_test_list_run_roundtrip() {
        let (_dir, engine) = engine();
        let saved = engine
            .save_skill("Greeter", "Greets the input", GREET_CODE, GREET_TEST)
            .unwrap();
        assert_eq!(saved.name, "greeter");
        assert_eq!(saved.version, 1);
        assert_eq!(saved.test_status, TestStatus::Passed);

        let listed = engine.list_skills().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "greeter");

        let output = engine.run_skill("greeter", "hello").unwrap();
        assert_eq!(output, "You said: hello");
    }

    #[test]
    fn failing_test_flags_skill_and_blocks_execution() {
        let (_dir, engine) = engine();
        let bad_test = r#"fn test() { run("hi") == "something else" }"#;
        let saved = engine
            .save_skill("broken", "always wrong", GREET_CODE, bad_test)
            .unwrap();
        assert!(matches!(saved.test_status, TestStatus::Failed(_)));

        match engine.run_skill("broken", "x") {
            Err(SkillError::Flagged(name, _)) => assert_eq!(name, "broken"),
            other => panic!("flagged skill must refuse to run, got {other:?}"),
        }
    }

    #[test]
    fn updating_bumps_version_and_archives_previous() {
        let (dir, engine) = engine();
        engine
            .save_skill("greeter", "v1", GREET_CODE, GREET_TEST)
            .unwrap();
        let v2_code = r#"fn run(input) { "V2: " + input }"#;
        let v2_test = r#"fn test() { run("x") == "V2: x" }"#;
        let updated = engine
            .save_skill("greeter", "v2", v2_code, v2_test)
            .unwrap();
        assert_eq!(updated.version, 2);
        assert_eq!(updated.test_status, TestStatus::Passed);

        let archived = dir.path().join("skills/greeter/history/v1/skill.rhai");
        let archived_code = std::fs::read_to_string(archived).unwrap();
        assert!(archived_code.contains("You said"), "v1 code is preserved");
        assert_eq!(engine.run_skill("greeter", "y").unwrap(), "V2: y");
    }

    #[test]
    fn runaway_skill_is_terminated_by_operation_cap() {
        let (_dir, engine) = engine();
        let infinite = r#"fn run(input) { let x = 0; loop { x += 1; } }"#;
        // Test must pass for run to be allowed, so give it a trivial test
        // that doesn't call run().
        let trivial_test = "fn test() { true }";
        engine
            .save_skill("spinner", "never halts", infinite, trivial_test)
            .unwrap();
        match engine.run_skill("spinner", "go") {
            Err(SkillError::Execution(msg)) => {
                assert!(
                    msg.to_lowercase().contains("operation")
                        || msg.to_lowercase().contains("terminat"),
                    "expected operation-cap termination, got: {msg}"
                );
            }
            other => panic!("runaway skill must be terminated, got {other:?}"),
        }
    }

    #[test]
    fn compile_errors_flag_instead_of_crash() {
        let (_dir, engine) = engine();
        let saved = engine
            .save_skill(
                "syntax",
                "broken source",
                "fn run(input) {",
                "fn test() { true }",
            )
            .unwrap();
        match saved.test_status {
            TestStatus::Failed(detail) => assert!(detail.contains("compile error")),
            other => panic!("expected compile failure flag, got {other:?}"),
        }
    }

    #[test]
    fn retest_persists_new_status() {
        let (dir, engine) = engine();
        engine
            .save_skill("greeter", "d", GREET_CODE, GREET_TEST)
            .unwrap();
        // Sabotage the code on disk, then retest: status must flip to failed.
        std::fs::write(
            dir.path().join("skills/greeter/skill.rhai"),
            r#"fn run(input) { "wrong" }"#,
        )
        .unwrap();
        let retested = engine.test_skill("greeter").unwrap();
        assert!(matches!(retested.test_status, TestStatus::Failed(_)));
        // And the flag persists for the next reader.
        let listed = engine.list_skills().unwrap();
        assert!(matches!(listed[0].test_status, TestStatus::Failed(_)));
    }
}
