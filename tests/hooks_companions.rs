pub mod support;
use support::Fixture;
#[path = "support/hooks.rs"]
pub mod helpers;

#[test]
fn setup_replaces_the_legacy_collector_and_preserves_foreign_handlers()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = helpers::configured_fixture()?;
    let legacy = format!(
        "'{}' measure hook --agent claude-code",
        fixture.home().join(".local/bin/arnes").display()
    );
    fixture.write_home(".claude/settings.json", &serde_json::json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":legacy},{"type":"command","command":"foreign-handler"}]}]}}).to_string())?;
    let (code, _, stderr) = helpers::run(&fixture, &["setup", "hooks", "--agent", "claude"])?;
    assert_eq!(code, 0, "{stderr}");
    let config = std::fs::read_to_string(fixture.home().join(".claude/settings.json"))?;
    assert!(!config.contains(" measure hook"));
    assert!(config.contains("agent-measure' hook --agent claude-code"));
    assert!(config.contains("foreign-handler"));
    Ok(())
}

#[test]
fn setup_migrates_legacy_claude_hooks() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    migrate_legacy_hooks("claude", ".claude/settings.json", "claude-code")
}

#[test]
fn setup_migrates_legacy_codex_hooks() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    migrate_legacy_hooks("codex", ".codex/hooks.json", "codex")
}

fn migrate_legacy_hooks(
    agent: &str,
    configuration: &str,
    measurement_agent: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for declared_output_discipline in [false, true] {
        let fixture = Fixture::new()?;
        let output_declaration = if declared_output_discipline {
            format!(
                "  - id: output-discipline\n    installations:\n      - {{agent: {agent}, scope: user}}\n"
            )
        } else {
            String::new()
        };
        fixture.write_home(
            ".arnes.yaml",
            &format!(
                "version: 1\nagents:\n  - id: {agent}\n    scopes: [user]\nhooks:\n  - id: measurement\n    installations:\n      - {{agent: {agent}, scope: user}}\n{output_declaration}resources: []\n"
            ),
        )?;
        helpers::executable(&fixture, "agent-measure")?;
        let output_path = helpers::executable(&fixture, "output-discipline-hook")?;
        let arnes = fixture.home().join(".local/bin/arnes");
        let legacy_measurement = format!(
            "'{}' measure hook --agent {measurement_agent}",
            arnes.display()
        );
        let legacy_output = format!("'{}' output-discipline", arnes.display());
        let foreign =
            serde_json::json!({"type":"command", "command":"foreign-handler", "timeout":7});
        fixture.write_home(
            configuration,
            &serde_json::json!({"hooks":{
                "SessionStart":[{"matcher":"startup|resume|clear|compact", "hooks":[
                    {"type":"command", "command":legacy_measurement},
                    {"type":"command", "command":legacy_output},
                    foreign
                ]}]
            }})
            .to_string(),
        )?;
        let (code, _, stderr) = helpers::run(&fixture, &["setup", "hooks", "--agent", agent])?;
        assert_eq!(code, 0, "{agent}: {stderr}");
        let path = fixture.home().join(configuration);
        let installed = std::fs::read_to_string(&path)?;
        let config: serde_json::Value = serde_json::from_str(&installed)?;
        let commands = config
            .get("hooks")
            .ok_or("missing hooks")?
            .as_object()
            .ok_or("expected hooks object")?
            .values()
            .filter_map(serde_json::Value::as_array)
            .flatten()
            .filter_map(|group| group.get("hooks").and_then(serde_json::Value::as_array))
            .flatten()
            .collect::<Vec<_>>();
        assert!(!commands.iter().any(|handler| {
            handler.get("command").and_then(serde_json::Value::as_str) == Some(&legacy_measurement)
        }));
        assert!(!commands.iter().any(|handler| {
            handler.get("command").and_then(serde_json::Value::as_str) == Some(&legacy_output)
        }));
        assert!(commands.contains(&&foreign));
        let output_command = format!("'{}'", output_path.display());
        assert_eq!(
            commands
                .iter()
                .filter(
                    |handler| handler.get("command").and_then(serde_json::Value::as_str)
                        == Some(&output_command)
                )
                .count(),
            usize::from(declared_output_discipline)
        );
        let before = fixture.snapshot()?;
        let (code, _, stderr) = helpers::run(&fixture, &["setup", "hooks", "--agent", agent])?;
        assert_eq!(code, 0, "{stderr}");
        assert_eq!(fixture.snapshot()?, before);
        doctor_without_foreign_handler(&fixture, agent, configuration, config)?;
    }
    Ok(())
}

fn doctor_without_foreign_handler(
    fixture: &Fixture,
    agent: &str,
    configuration: &str,
    mut config: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let groups = config
        .pointer_mut("/hooks/SessionStart")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("missing SessionStart groups")?;
    for group in groups {
        let handlers = group
            .get_mut("hooks")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or("missing handlers")?;
        handlers.retain(|handler| {
            handler.get("command").and_then(serde_json::Value::as_str) != Some("foreign-handler")
        });
    }
    fixture.write_home(configuration, &config.to_string())?;
    let (code, stdout, stderr) = helpers::run(fixture, &["doctor", "hooks", "--agent", agent])?;
    assert_eq!(code, 0, "{agent}: {stdout}\n{stderr}");
    Ok(())
}

#[test]
fn output_discipline_uses_its_own_companion_without_requiring_measurement()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = Fixture::new()?;
    fixture.write_home(".arnes.yaml", "version: 1\nagents:\n  - id: claude\n    scopes: [user]\nhooks:\n  - id: output-discipline\n    installations:\n      - {agent: claude, scope: user}\nresources: []\n")?;
    let (code, _, _) = helpers::run(&fixture, &["setup", "hooks", "--agent", "claude"])?;
    assert_eq!(code, 2);
    assert!(!fixture.home().join(".claude/settings.json").exists());
    helpers::executable(&fixture, "output-discipline-hook")?;
    let (code, _, stderr) = helpers::run(&fixture, &["setup", "hooks", "--agent", "claude"])?;
    assert_eq!(code, 0, "{stderr}");
    let config = std::fs::read_to_string(fixture.home().join(".claude/settings.json"))?;
    assert!(config.contains("output-discipline-hook'"));
    assert!(!config.contains("agent-measure"));
    Ok(())
}

#[test]
fn hook_setup_refuses_to_replace_the_explicit_manifest()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = helpers::configured_fixture()?;
    fixture.write_home(
        ".claude/settings.json",
        r#"{"version":1,"agents":[{"id":"claude","scopes":["user"]}],"resources":[]}"#,
    )?;
    let manifest = fixture.home().join(".claude/settings.json");
    let before = fixture.snapshot()?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_arnes"))
        .args(["setup", "hooks", "--agent", "claude"])
        .current_dir(fixture.repository())
        .env_clear()
        .env("HOME", fixture.home())
        .env("ARNES_MANIFEST", &manifest)
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fixture.snapshot()?, before);
    Ok(())
}
