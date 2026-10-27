use std::{fs, process::Command};

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
fn explicit_roots_accept_custom_skills_and_required_plugins_from_a_symlinked_manifest()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("source-bundle");
    let repository = fixture.path().join("project");
    let home = fixture.path().join("consumer");
    let checkout = fixture.path().join("manifest-checkout");
    for directory in [&source, &repository, &home, &checkout] {
        fs::create_dir(directory)?;
    }
    fs::create_dir_all(source.join("vendor/custom"))?;
    fs::write(source.join("vendor/custom/SKILL.md"), "# Custom\n")?;
    fs::create_dir_all(repository.join("vendor/custom"))?;
    fs::write(
        repository.join("vendor/custom/SKILL.md"),
        "# Repository decoy\n",
    )?;
    fs::create_dir_all(checkout.join("vendor/custom"))?;
    fs::write(
        checkout.join("vendor/custom/SKILL.md"),
        "# Checkout decoy\n",
    )?;
    let manifest = checkout.join(".arnes.yaml");
    fs::write(
        &manifest,
        "version: 1
agents:
  - id: claude
    scopes: [user]
skills:
  - slug: alpha
    source: vendor/custom
    installations: [{ agent: claude, scope: user }]
external:
  plugins:
    - { agent: claude, scope: user, id: example, required: true }
resources:
  - id: skills
    kind: skills
    agent: claude
    scope: user
    layout: leaves
    source: { root: repository, path: harness/skills }
    destination: { root: home, path: .claude/skills }
",
    )?;
    std::os::unix::fs::symlink(&manifest, home.join(".arnes.yaml"))?;
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_arnes"))
            .args(args)
            .current_dir(fixture.path())
            .env_clear()
            .env("HOME", &home)
            .env("ARNES_SOURCE_ROOT", &source)
            .env("ARNES_REPOSITORY_ROOT", &repository)
            .output()
    };
    fs::create_dir_all(home.join(".claude/skills"))?;
    std::os::unix::fs::symlink(
        source.join("vendor/custom"),
        home.join(".claude/skills/alpha"),
    )?;
    let output = invoke(&["doctor", "manifest"])?;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_skill_state(
        &invoke(&[
            "doctor", "skills", "--agent", "claude", "--scope", "user", "--format", "json",
        ])?,
        "healthy",
    )?;
    fs::write(
        source.join("vendor/custom/SKILL.md"),
        "[missing](missing.md)\n",
    )?;
    assert_skill_state(
        &invoke(&[
            "doctor", "skills", "--agent", "claude", "--scope", "user", "--format", "json",
        ])?,
        "drift",
    )?;
    Ok(())
}

fn assert_skill_state(
    output: &std::process::Output,
    state: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
    let skill = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .get("message")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|message| message.contains("managed claude user skill alpha"))
        })
        .ok_or("missing managed skill diagnostic")?;
    assert_eq!(
        skill.get("state").and_then(serde_json::Value::as_str),
        Some(state),
        "{diagnostics:?}"
    );
    Ok(())
}
