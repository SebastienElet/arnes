#![cfg(test)]
#[path = "support/hooks.rs"]
pub mod hook_support;
pub mod support;
use hook_support::{
    MEASUREMENT_ONLY_MANIFEST, configured_fixture, executable, installed_fixture, run, settings,
    settings_path, write_settings,
};
use serde_json::{Value, json};
use std::fs;
fn doctor(
    fixture: &support::Fixture,
) -> Result<(i32, String, String), Box<dyn std::error::Error + Send + Sync>> {
    run(fixture, &["doctor", "hooks", "--agent", "claude", "-v"])
}
#[test]
fn declared_installed_hooks_are_healthy() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let (code, stdout, stderr) = doctor(&fixture)?;
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("healthy hooks: claude user measurement hook is installed on 14 events"),
        "{stdout}"
    );
    assert!(
        stdout.contains("healthy hooks: claude user handoff hook is installed on Stop"),
        "{stdout}"
    );
    assert_eq!(stderr, "");
    Ok(())
}
#[test]
fn declared_installed_memory_hook_is_healthy()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = support::Fixture::new()?;
    fixture.write_home(".arnes.yaml", hook_support::MEMORY_MANIFEST)?;
    executable(&fixture, "agent-measure")?;
    executable(&fixture, "agent-memory")?;
    let command = format!(
        "'{}' hook --agent claude",
        fixture.home().join(".local/bin/agent-memory").display()
    );
    write_settings(
        &fixture,
        &json!({"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":command}]}]}}),
    )?;
    let (code, stdout, stderr) = doctor(&fixture)?;
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("healthy hooks: claude user memory hook is installed on UserPromptSubmit"),
        "{stdout}"
    );
    assert_eq!(stderr, "");
    Ok(())
}
#[test]
fn a_missing_configuration_file_is_drift() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = configured_fixture()?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains("claude user hook configuration ~/.claude/settings.json is missing"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn a_removed_event_is_drift() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let mut config = settings(&fixture)?;
    (*(config)
        .get_mut("hooks")
        .ok_or("missing fixture index hooks")?)
    .as_object_mut()
    .ok_or("expected JSON object")?
    .remove("PreToolUse");
    write_settings(&fixture, &config)?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 1);
    assert!(
        stdout.contains("claude user measurement hook is missing from PreToolUse"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn an_unexpected_event_is_drift() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let mut config = settings(&fixture)?;
    let command = measurement_command(&config)?;
    {
        (config)
            .get_mut("hooks")
            .ok_or("missing fixture index hooks")?
            .as_object_mut()
            .ok_or("expected JSON object for fixture update")?
            .insert(
                ("Notification").to_owned(),
                json ! ([{ "hooks" : [{ "type" : "command" , "command" : command }] }]),
            );
    };
    write_settings(&fixture, &config)?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 1);
    assert!(
        stdout.contains(
            "claude user measurement hook is installed on unexpected events Notification"
        ),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn an_installed_hook_without_a_declaration_is_drift()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    fixture.write_home(".arnes.yaml", MEASUREMENT_ONLY_MANIFEST)?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 1);
    assert!(
        stdout.contains("claude user handoff hook is installed but not declared"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn a_missing_hook_command_is_drift() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    fs::remove_file(fixture.home().join(".local/bin/agent-measure"))?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 1);
    assert!(
        stdout
            .contains("claude user measurement hook command ~/.local/bin/agent-measure is missing"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn a_non_executable_hook_command_is_an_error()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    fs::set_permissions(
        fixture.home().join(".local/bin/agent-measure"),
        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
    )?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 2);
    assert!(
        stdout.contains(
            "claude user measurement hook command ~/.local/bin/agent-measure is not an executable file"
        ),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn a_malformed_configuration_is_an_error() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = configured_fixture()?;
    fs::create_dir_all(
        settings_path(&fixture)
            .parent()
            .ok_or("fixture path has no parent")?,
    )?;
    fs::write(settings_path(&fixture), "{")?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 2);
    assert!(
        stdout.contains("claude user hook configuration ~/.claude/settings.json is malformed"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn an_invalid_configuration_is_an_error() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = configured_fixture()?;
    write_settings(&fixture, &json ! ({ "hooks" : { "Stop" : "oops" } }))?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 2);
    assert!(
        stdout.contains("claude user hook configuration ~/.claude/settings.json is invalid"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Claude hook event must be an array"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn project_scope_hooks_are_unsupported() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let (code, stdout, _) = run(
        &fixture,
        &["doctor", "hooks", "--agent", "claude", "--scope", "project"],
    )?;
    assert_eq!(code, 0);
    assert!(
        stdout.contains("claude project hooks are not supported"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn agents_without_declared_hooks_are_unsupported()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let (code, stdout, _) = run(&fixture, &["doctor", "hooks", "--agent", "cursor"])?;
    assert_eq!(code, 0);
    assert!(
        stdout.contains("cursor user hooks are not declared"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn undeclared_agents_are_unsupported() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let (code, stdout, _) = run(&fixture, &["doctor", "hooks", "--agent", "codex"])?;
    assert_eq!(code, 0);
    assert!(
        stdout.contains(
            "unsupported hooks: codex user hook installations are not declared or supported"
        ),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn hook_diagnostics_use_the_hooks_resource() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let fixture = installed_fixture()?;
    let (code, stdout, _) = run(
        &fixture,
        &["doctor", "hooks", "--agent", "claude", "--format", "json"],
    )?;
    assert_eq!(code, 0);
    let diagnostics: Value = serde_json::from_str(&stdout)?;
    assert_eq!(
        *(*(diagnostics).get(0).ok_or("missing fixture index 0")?)
            .get("resource")
            .ok_or("missing fixture index resource")?,
        "hooks"
    );
    assert_eq!(
        *(*(diagnostics).get(0).ok_or("missing fixture index 0")?)
            .get("state")
            .ok_or("missing fixture index state")?,
        "healthy"
    );
    Ok(())
}
#[test]
fn the_hooks_doctor_is_read_only() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let before = fixture.snapshot()?;
    let (code, _, _) = run(&fixture, &["doctor", "hooks"])?;
    assert_eq!(code, 0);
    assert_eq!(fixture.snapshot()?, before);
    Ok(())
}
#[test]
fn the_default_doctor_reports_hooks() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = configured_fixture()?;
    let (code, stdout, _) = run(&fixture, &["doctor"])?;
    assert_eq!(code, 1);
    assert!(stdout.contains("Hooks"), "{stdout}");
    assert!(
        stdout.contains("claude user hook configuration ~/.claude/settings.json is missing"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn direct_hook_entries_are_recognised() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = support::Fixture::new()?;
    fixture.write_home(".arnes.yaml", hook_support::CURSOR_MANIFEST)?;
    hook_support::executable(&fixture, "agent-measure")?;
    fixture.write_home(
        ".cursor/hooks.json",
        &hook_support::cursor_hooks(&fixture).to_string(),
    )?;
    let (code, stdout, _) = run(&fixture, &["doctor", "hooks", "--agent", "cursor", "-v"])?;
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("healthy hooks: cursor user measurement hook is installed on 12 events"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn a_superseded_handoff_command_is_drift() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = hook_support::linked_handoff_fixture()?;
    let mut config = settings(&fixture)?;
    let superseded = hook_support::superseded_handoff_command(&fixture)?;
    (*(*(config)
        .get_mut("hooks")
        .ok_or("missing fixture index hooks")?)
    .get_mut("Stop")
    .ok_or("missing fixture index Stop")?)
    .as_array_mut()
    .ok_or("required test value is missing")?
    .push(json ! ({ "hooks" : [{ "type" : "command" , "command" : superseded }] }));
    write_settings(&fixture, &config)?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains("claude user handoff hook is installed with superseded commands"),
        "{stdout}"
    );
    Ok(())
}
#[test]
fn a_command_outside_every_hook_kind_is_drift()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = installed_fixture()?;
    let mut config = settings(&fixture)?;
    let hooks = config
        .get_mut("hooks")
        .ok_or("missing fixture index hooks")?
        .as_object_mut()
        .ok_or("expected JSON object for fixture update")?;
    hooks.insert(
        "Notification".to_owned(),
        json!([{ "hooks": [{ "type": "command", "command": "/opt/legacy-hook" }] }]),
    );
    hooks
        .get_mut("Stop")
        .ok_or("missing fixture index Stop")?
        .as_array_mut()
        .ok_or("required test value is missing")?
        .push(json!({ "hooks": [{ "type": "command", "command": "/opt/legacy-hook" }] }));
    write_settings(&fixture, &config)?;
    let (code, stdout, _) = doctor(&fixture)?;
    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains(
            "claude user hook command /opt/legacy-hook is installed on Notification, Stop but not declared"
        ),
        "{stdout}"
    );
    Ok(())
}
fn measurement_command(config: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(
        (*(*(*(*(*(*(config).get("hooks").ok_or("missing fixture index hooks")?)
            .get("PreToolUse")
            .ok_or("missing fixture index PreToolUse")?)
        .get(0)
        .ok_or("missing fixture index 0")?)
        .get("hooks")
        .ok_or("missing fixture index hooks")?)
        .get(0)
        .ok_or("missing fixture index 0")?)
        .get("command")
        .ok_or("missing fixture index command")?)
        .as_str()
        .ok_or("expected JSON string")?
        .to_owned(),
    )
}

#[test]
fn legacy_hooks_are_reported_without_migration()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for (agent, path) in [
        ("claude", ".claude/settings.json"),
        ("codex", ".codex/hooks.json"),
        ("cursor", ".cursor/hooks.json"),
    ] {
        let fixture = support::Fixture::new()?;
        fixture.write_home(
            ".arnes.yaml",
            &format!("version: 1\nagents:\n  - {{id: {agent}, scopes: [user]}}\nresources: []\n"),
        )?;
        let legacy = format!(
            "'{}' measure hook --agent {agent}",
            fixture.home().join(".local/bin/arnes").display()
        );
        let output_discipline = format!(
            "'{}'",
            fixture
                .home()
                .join(".local/bin/output-discipline-hook")
                .display()
        );
        let handlers = json!([{"type":"command", "command":legacy}, {"type":"command", "command":output_discipline}]);
        let config = if agent == "cursor" {
            json!({"version":1,"hooks":{"stop":handlers}})
        } else {
            json!({"hooks":{"Stop":[{"hooks":handlers}]}})
        };
        fixture.write_home(path, &config.to_string())?;
        let before = fixture.snapshot()?;
        let (code, stdout, stderr) = run(
            &fixture,
            &["doctor", "hooks", "--agent", agent, "--format", "json"],
        )?;
        assert_eq!(code, 1, "{stdout}");
        assert!(stdout.contains("measure hook"), "{stdout}");
        assert!(
            stdout.contains("output-discipline hook is installed but not declared"),
            "{stdout}"
        );
        assert!(stderr.is_empty());
        assert_eq!(fixture.snapshot()?, before);
    }
    Ok(())
}
