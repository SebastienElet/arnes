use std::{fs, process::Command};

#[test]
fn sync_uses_explicit_sources_without_a_dotfiles_layout() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("sources");
    let home = fixture.path().join("consumer");
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&home)?;
    fs::write(source.join("rules.md"), "shared rule\n")?;
    let manifest = fixture.path().join("selection.yaml");
    fs::write(
        &manifest,
        "version: 1\nagents:\n  - id: claude\n    scopes: [user]\nresources:\n  - id: rule\n    kind: rules\n    agent: claude\n    scope: user\n    source: { root: repository, path: rules.md }\n    destination: { root: home, path: .claude/rules/example.md }\n",
    )?;
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_arnes"))
            .args(["sync", "rules", "--agent", "claude", "--scope", "user"])
            .current_dir(fixture.path())
            .env_clear()
            .env("HOME", &home)
            .env("ARNES_SOURCE_ROOT", &source)
            .env("ARNES_REPOSITORY_ROOT", &source)
            .env("ARNES_MANIFEST", &manifest)
            .output()
    };
    let output = invoke()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(home.join(".claude/rules/example.md"))?,
        "shared rule\n"
    );
    fs::remove_file(home.join(".claude/rules/example.md"))?;
    fs::write(home.join(".claude/rules/example.md"), "foreign\n")?;
    assert!(!invoke()?.status.success());
    assert_eq!(
        fs::read_to_string(home.join(".claude/rules/example.md"))?,
        "foreign\n"
    );
    Ok(())
}

#[test]
fn explicit_sources_reject_relative_missing_and_empty_roots()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    fs::write(
        fixture.path().join(".arnes.yaml"),
        "version: 1\nagents: []\nresources: []\n",
    )?;
    for root in ["", "relative", "/missing-arnes-source-552"] {
        let output = Command::new(env!("CARGO_BIN_EXE_arnes"))
            .args(["doctor", "manifest"])
            .current_dir(fixture.path())
            .env_clear()
            .env("HOME", fixture.path())
            .env("ARNES_SOURCE_ROOT", root)
            .output()?;
        assert_eq!(output.status.code(), Some(2), "{root:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("ARNES_SOURCE_ROOT"));
    }
    Ok(())
}

#[test]
fn sync_refuses_to_overwrite_its_explicit_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    fs::create_dir(fixture.path().join(".claude"))?;
    fs::create_dir(fixture.path().join("source"))?;
    let manifest = fixture.path().join(".claude/settings.json");
    let contents = r#"{"version":1,"agents":[{"id":"claude","scopes":["user"],"user_config":{"model":"expected"}}],"resources":[]}"#;
    fs::write(&manifest, contents)?;
    let output = Command::new(env!("CARGO_BIN_EXE_arnes"))
        .args(["sync", "config", "--agent", "claude", "--scope", "user"])
        .current_dir(fixture.path().join("source"))
        .env_clear()
        .env("HOME", fixture.path())
        .env("ARNES_MANIFEST", &manifest)
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(manifest)?, contents);
    Ok(())
}
