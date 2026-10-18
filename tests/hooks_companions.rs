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
