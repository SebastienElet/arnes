use crate::support::Fixture;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
pub const MANIFEST: &str = "version: 1
agents:
  - id: claude
    scopes: [user, project]
  - id: cursor
    scopes: [user]
hooks:
  - id: measurement
    installations:
      - { agent: claude, scope: user }
  - id: handoff
    installations:
      - { agent: claude, scope: user }
resources: []
";
pub const CURSOR_MANIFEST: &str = "version: 1
agents:
  - id: cursor
    scopes: [user]
hooks:
  - id: measurement
    installations:
      - { agent: cursor, scope: user }
resources: []
";
pub const MEASUREMENT_ONLY_MANIFEST: &str = "version: 1
agents:
  - id: claude
    scopes: [user, project]
hooks:
  - id: measurement
    installations:
      - { agent: claude, scope: user }
resources: []
";
pub const MEMORY_MANIFEST: &str = "version: 1
agents:
  - id: claude
    scopes: [user]
hooks:
  - id: memory
    installations:
      - { agent: claude, scope: user }
resources: []
";
/// # Errors
/// Returns an error if the fixture files, directories or symbolic links cannot be prepared.
pub fn configured_fixture() -> Result<Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = Fixture::new()?;
    fixture.write_home(".arnes.yaml", MANIFEST)?;
    executable(&fixture, "agent-measure")?;
    executable(&fixture, "agent-handoff")?;
    Ok(fixture)
}
/// # Errors
/// Returns an error if the fixture files, directories or symbolic links cannot be prepared.
pub fn installed_fixture() -> Result<Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = configured_fixture()?;
    write_settings(&fixture, &claude_hooks(&fixture))?;
    Ok(fixture)
}
/// # Errors
/// Returns an error if the fixture files, directories or symbolic links cannot be prepared.
pub fn linked_handoff_fixture() -> Result<Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = Fixture::new()?;
    fixture.write_home(".arnes.yaml", MANIFEST)?;
    executable(&fixture, "agent-measure")?;
    let target = fixture.repository().join("tooling/agent-handoff");
    fs::create_dir_all(target.parent().ok_or("fixture path has no parent")?)?;
    fs::write(&target, b"binary")?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700))?;
    let alias = fixture.home().join(".local/bin/agent-handoff");
    fs::create_dir_all(alias.parent().ok_or("fixture path has no parent")?)?;
    std::os::unix::fs::symlink(&target, &alias)?;
    write_settings(&fixture, &claude_hooks(&fixture))?;
    Ok(fixture)
}
/// # Errors
/// Returns an error if the repository cannot be canonicalized or its path is not UTF-8.
pub fn superseded_handoff_command(
    fixture: &Fixture,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(fs::canonicalize(fixture.repository())?
        .join("scripts/agent_handoff")
        .to_str()
        .ok_or("required test value is missing")?
        .to_owned())
}
/// # Errors
/// Returns an error if the fixture executable cannot be written or its permissions cannot be set.
pub fn executable(
    fixture: &Fixture,
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let path = fixture.home().join(".local/bin").join(name);
    fs::create_dir_all(path.parent().ok_or("fixture path has no parent")?)?;
    fs::write(&path, b"binary")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}
/// # Errors
/// Returns an error if the settings file cannot be read or parsed as JSON.
pub fn settings(fixture: &Fixture) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(serde_json::from_slice(&fs::read(settings_path(fixture))?)?)
}
/// # Errors
/// Returns an error if settings cannot be serialized or written.
pub fn write_settings(
    fixture: &Fixture,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = settings_path(fixture);
    fs::create_dir_all(path.parent().ok_or("fixture path has no parent")?)?;
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
#[must_use]
pub fn settings_path(fixture: &Fixture) -> PathBuf {
    fixture.home().join(".claude/settings.json")
}
/// # Errors
/// Returns an error if the fixture command, filesystem inspection or output decoding fails.
pub fn run(
    fixture: &Fixture,
    args: &[&str],
) -> Result<(i32, String, String), Box<dyn std::error::Error + Send + Sync>> {
    let output = fixture.command(args)?;
    Ok((
        output
            .status
            .code()
            .ok_or("child process exited without a status code")?,
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

#[must_use]
pub fn claude_hooks(fixture: &Fixture) -> Value {
    let command = format!(
        "'{}' hook --agent claude-code",
        fixture.home().join(".local/bin/agent-measure").display()
    );
    let mut hooks = serde_json::Map::new();
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PermissionRequest",
        "PermissionDenied",
        "PostToolUse",
        "PostToolUseFailure",
        "SubagentStart",
        "SubagentStop",
        "Stop",
        "StopFailure",
        "PreCompact",
        "PostCompact",
        "SessionEnd",
    ] {
        hooks.insert(
            event.to_owned(),
            json!([{"hooks":[{"type":"command","command":command}]}]),
        );
    }
    if let Some(stop) = hooks
        .get_mut("Stop")
        .and_then(Value::as_array_mut)
        .and_then(|entries| entries.first_mut())
        .and_then(|entry| entry.get_mut("hooks"))
        .and_then(Value::as_array_mut)
    {
        stop.push(json!({"type":"command","command":fixture.home().join(".local/bin/agent-handoff").display().to_string()}));
    }
    json!({"hooks":hooks})
}

#[must_use]
pub fn cursor_hooks(fixture: &Fixture) -> Value {
    let command = format!(
        "'{}' hook --agent cursor",
        fixture.home().join(".local/bin/agent-measure").display()
    );
    let hooks = [
        "sessionStart",
        "beforeSubmitPrompt",
        "preToolUse",
        "postToolUse",
        "postToolUseFailure",
        "subagentStart",
        "subagentStop",
        "afterAgentResponse",
        "stop",
        "preCompact",
        "postCompact",
        "sessionEnd",
    ]
    .into_iter()
    .map(|event| (event.to_owned(), json!([{"command":command}])))
    .collect::<serde_json::Map<_, _>>();
    json!({"version":1,"hooks":hooks})
}
