#![cfg(test)]
#[path = "support/hooks.rs"]
pub mod hook_support;
pub mod support;
use hook_support::{executable, run};
use serde_json::json;
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
  - id: semctx-nudge
    installations:
      - { agent: claude, scope: user }
      - { agent: codex, scope: user }
resources: []
";

fn fixture(manifest: &str) -> Result<support::Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = support::Fixture::new()?;
    fixture.write_home(".arnes.yaml", manifest)?;
    executable(&fixture, "agent-measure")?;
    let tool = fixture.repository().join("tooling/semctx-nudge");
    fs::create_dir_all(tool.parent().ok_or("fixture path has no parent")?)?;
    fs::write(&tool, b"tool")?;
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700))?;
    Ok(fixture)
}

fn tool_path(
    fixture: &support::Fixture,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let path = fs::canonicalize(fixture.repository())?.join("tooling/semctx-nudge");
    Ok(path
        .to_str()
        .ok_or("required test value is missing")?
        .to_owned())
}

fn command(
    fixture: &support::Fixture,
    host: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(format!(
        "'{}' --host {host}",
        tool_path(fixture)?.replace('\'', "'\\''")
    ))
}

fn prepare_installed_hook(fixture: &support::Fixture, agent: &str) -> TestResult {
    let command = command(fixture, agent)?;
    let matcher = "startup|resume|clear|compact";
    let path = if agent == "codex" {
        ".codex/hooks.json"
    } else {
        ".claude/settings.json"
    };
    fixture.write_home(path, &json!({"hooks":{"SessionStart":[{"matcher":matcher,"hooks":[{"type":"command","command":command,"timeout":5}]}]}}).to_string())?;
    Ok(())
}

#[test]
fn doctor_reports_the_installed_hook_as_healthy() -> TestResult {
    let fixture = fixture(MANIFEST)?;
    prepare_installed_hook(&fixture, "codex")?;

    let (code, stdout, _) = run(&fixture, &["doctor", "hooks", "--agent", "codex", "-v"])?;

    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("healthy hooks: codex user semctx-nudge hook is installed on SessionStart"),
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
        stdout.contains("claude user semctx-nudge hook is missing from SessionStart"),
        "{stdout}"
    );
    Ok(())
}
