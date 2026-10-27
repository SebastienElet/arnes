#![cfg(test)]
#[path = "support/hooks.rs"]
pub mod hook_support;
pub mod support;
use hook_support::{executable, run};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const MANIFEST: &str = "version: 1
agents:
  - id: claude
    scopes: [user]
  - id: codex
    scopes: [user]
hooks:
  - id: format-edited-file
    installations:
      - { agent: claude, scope: user }
      - { agent: codex, scope: user }
resources: []
";

fn fixture(manifest: &str) -> Result<support::Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = support::Fixture::new()?;
    fixture.write_home(".arnes.yaml", manifest)?;
    executable(&fixture, "agent-measure")?;
    let tool = fixture.repository().join("tooling/format-edited-file");
    fs::create_dir_all(tool.parent().ok_or("fixture path has no parent")?)?;
    fs::write(&tool, b"tool")?;
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700))?;
    Ok(fixture)
}

fn command(fixture: &support::Fixture) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let path = fs::canonicalize(fixture.repository())?.join("tooling/format-edited-file");
    let path = path.to_str().ok_or("required test value is missing")?;
    Ok(format!("'{}'", path.replace('\'', "'\\''")))
}

fn config(
    fixture: &support::Fixture,
    relative: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(serde_json::from_slice(&fs::read(
        fixture.home().join(relative),
    )?)?)
}

fn prepare_installed_hook(fixture: &support::Fixture, agent: &str) -> TestResult {
    let command = command(fixture)?;
    let matcher = if agent == "codex" {
        "apply_patch"
    } else {
        "Edit|Write|MultiEdit"
    };
    let path = if agent == "codex" {
        ".codex/hooks.json"
    } else {
        ".claude/settings.json"
    };
    fixture.write_home(path, &json!({"hooks":{"PostToolUse":[{"matcher":matcher,"hooks":[{"type":"command","command":command,"timeout":30}]}]}}).to_string())?;
    Ok(())
}

#[test]
fn doctor_reports_the_installed_hook_as_healthy() -> TestResult {
    let fixture = fixture(MANIFEST)?;
    prepare_installed_hook(&fixture, "claude")?;

    let (code, stdout, _) = run(&fixture, &["doctor", "hooks", "--agent", "claude", "-v"])?;

    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains(
            "healthy hooks: claude user format-edited-file hook is installed on PostToolUse"
        ),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn doctor_reports_a_changed_matcher_as_drift() -> TestResult {
    let fixture = fixture(MANIFEST)?;
    prepare_installed_hook(&fixture, "claude")?;
    let path = fixture.home().join(".claude/settings.json");
    let mut settings = config(&fixture, ".claude/settings.json")?;
    *settings
        .pointer_mut("/hooks/PostToolUse/0/matcher")
        .ok_or("missing fixture matcher")? = json!("Write");
    fs::write(&path, serde_json::to_vec_pretty(&settings)?)?;

    let (code, stdout, _) = run(&fixture, &["doctor", "hooks", "--agent", "claude", "-v"])?;

    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains(
            "claude user format-edited-file hook has incorrect PostToolUse matcher or execution settings"
        ),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn doctor_reports_an_undeployed_hook_as_missing() -> TestResult {
    let fixture = fixture(MANIFEST)?;
    fixture.write_home(".claude/settings.json", "{}")?;

    let (code, stdout, _) = run(&fixture, &["doctor", "hooks", "--agent", "claude", "-v"])?;

    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains("claude user format-edited-file hook is missing from PostToolUse"),
        "{stdout}"
    );
    Ok(())
}
