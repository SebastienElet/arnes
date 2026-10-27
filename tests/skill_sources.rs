#![cfg(test)]
pub mod support;

use std::fs;
use std::os::unix::fs::symlink;
use support::Fixture;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const MANIFEST: &str = "version: 1
agents:
  - id: claude
    scopes: [user]
  - id: cursor
    scopes: [user]
  - id: codex
    scopes: [user]
skills:
  - slug: alpha
    source: vendor/skills/custom
    installations:
      - { agent: claude, scope: user }
      - { agent: cursor, scope: user }
      - { agent: codex, scope: user }
resources:
  - id: claude-skills
    kind: skills
    agent: claude
    scope: user
    layout: leaves
    source: { root: repository, path: harness/skills }
    destination: { root: home, path: .claude/skills }
  - id: cursor-skills
    kind: skills
    agent: cursor
    scope: user
    layout: leaves
    source: { root: repository, path: harness/skills }
    destination: { root: home, path: .cursor/skills }
  - id: codex-skills
    kind: skills
    agent: codex
    scope: user
    layout: leaves
    source: { root: repository, path: harness/skills }
    destination: { root: home, path: .agents/skills }
";

fn fixture() -> Result<Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = Fixture::new()?;
    fixture.write_home(".arnes.yaml", MANIFEST)?;
    fixture.write_repository(
        "vendor/skills/custom/SKILL.md",
        "# Custom\n[guide](guide.md)\n",
    )?;
    fixture.write_repository("vendor/skills/custom/guide.md", "guide\n")?;
    Ok(fixture)
}

#[test]
fn doctor_checks_the_selected_custom_source_under_the_slug() -> TestResult {
    for (agent, destination) in [
        ("claude", ".claude/skills/alpha"),
        ("cursor", ".cursor/skills/alpha"),
        ("codex", ".agents/skills/alpha"),
    ] {
        let fixture = fixture()?;
        let destination = fixture.home().join(destination);
        fs::create_dir_all(destination.parent().ok_or("missing parent")?)?;
        symlink(
            fixture.repository().join("vendor/skills/custom"),
            destination,
        )?;
        let before = fixture.snapshot()?;
        let doctor = || fixture.command(["doctor", "skills", "--agent", agent, "--scope", "user"]);
        let output = doctor()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(fixture.snapshot()?, before);
        fs::remove_file(fixture.repository().join("vendor/skills/custom/guide.md"))?;
        assert_eq!(doctor()?.status.code(), Some(1));
    }
    Ok(())
}

#[test]
fn custom_source_paths_use_the_manifest_path_validation() -> TestResult {
    for source in ["/absolute", "../parent", "vendor/../parent", "''", "."] {
        let fixture = fixture()?;
        fixture.write_home(
            ".arnes.yaml",
            &MANIFEST.replace("vendor/skills/custom", source),
        )?;
        let before = fixture.snapshot()?;
        let output = fixture.command(["doctor", "manifest"])?;
        assert_eq!(output.status.code(), Some(2), "{source}");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("skills[0].source"), "{stdout}");
        assert!(
            stdout.contains("path cannot be empty")
                || stdout.contains("path must stay within its declared root"),
            "{stdout}"
        );
        assert_eq!(fixture.snapshot()?, before);
    }
    Ok(())
}

#[test]
fn custom_source_symlinks_within_the_source_root_are_supported() -> TestResult {
    let fixture = fixture()?;
    symlink("custom", fixture.repository().join("vendor/skills/alias"))?;
    fixture.write_home(
        ".arnes.yaml",
        &MANIFEST.replace("vendor/skills/custom", "vendor/skills/alias"),
    )?;
    fs::create_dir_all(fixture.home().join(".claude/skills"))?;
    symlink(
        fixture.repository().join("vendor/skills/alias"),
        fixture.home().join(".claude/skills/alpha"),
    )?;
    let output = fixture.command(["doctor", "skills", "--agent", "claude", "--scope", "user"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(())
}

#[test]
fn invalid_custom_sources_are_diagnosed_without_writing() -> TestResult {
    for kind in [
        "missing",
        "file",
        "dangling",
        "outside",
        "parent-outside",
        "roundtrip",
        "loop",
        "reference",
        "skill-file",
    ] {
        let fixture = fixture()?;
        let outside = tempfile::tempdir()?;
        fs::write(outside.path().join("SKILL.md"), "# Outside\n")?;
        let source = fixture.repository().join("vendor/skills/custom");
        match kind {
            "missing" => fs::remove_dir_all(&source)?,
            "file" => {
                fs::remove_dir_all(&source)?;
                fs::write(&source, "file\n")?;
            }
            "dangling" | "outside" | "roundtrip" | "loop" => {
                fs::remove_dir_all(&source)?;
                let target = match kind {
                    "outside" => outside.path().to_path_buf(),
                    "roundtrip" => {
                        fixture.write_repository("valid/SKILL.md", "# Valid\n")?;
                        symlink(
                            fixture.repository().join("valid"),
                            outside.path().join("return"),
                        )?;
                        outside.path().join("return")
                    }
                    "loop" => source.clone(),
                    _ => source.with_file_name("missing"),
                };
                symlink(target, &source)?;
            }
            "parent-outside" => {
                fs::remove_dir_all(fixture.repository().join("vendor"))?;
                fs::create_dir_all(outside.path().join("skills/custom"))?;
                fs::write(outside.path().join("skills/custom/SKILL.md"), "# Outside\n")?;
                symlink(outside.path(), fixture.repository().join("vendor"))?;
            }
            "reference" | "skill-file" => {
                let path = source.join(if kind == "reference" {
                    "guide.md"
                } else {
                    "SKILL.md"
                });
                fs::remove_file(&path)?;
                symlink(outside.path().join("SKILL.md"), path)?;
            }
            _ => return Err("unknown fixture".into()),
        }
        let destination = fixture.home().join(".claude/skills/alpha");
        fs::create_dir_all(destination.parent().ok_or("parent")?)?;
        symlink(&source, &destination)?;
        let outside_before = fs::read(outside.path().join("SKILL.md"))?;
        let before = fixture.snapshot()?;
        let output =
            fixture.command(["doctor", "skills", "--agent", "claude", "--scope", "user"])?;
        assert_eq!(
            output.status.code(),
            Some(2),
            "{kind}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(fixture.snapshot()?, before);
        assert_eq!(fs::read(outside.path().join("SKILL.md"))?, outside_before);
        assert_eq!(fs::read_link(&destination)?, source);
    }
    Ok(())
}

#[test]
fn doctor_allows_references_to_other_declared_custom_sources() -> TestResult {
    let fixture = fixture()?;
    fixture.write_home(
        ".arnes.yaml",
        &MANIFEST.replace(
            "resources:",
            r"  - slug: beta
    source: vendor/other
    installations: [{ agent: codex, scope: user }]
resources:",
        ),
    )?;
    fixture.write_repository("vendor/other/SKILL.md", "# Other\n")?;
    fixture.write_repository(
        "vendor/skills/custom/guide.md",
        "[other](../../other/SKILL.md)\n",
    )?;
    let destination = fixture.home().join(".claude/skills/alpha");
    fs::create_dir_all(destination.parent().ok_or("parent")?)?;
    symlink(
        fixture.repository().join("vendor/skills/custom"),
        destination,
    )?;
    let before = fixture.snapshot()?;
    let output = fixture.command(["doctor", "skills", "--agent", "claude", "--scope", "user"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fixture.snapshot()?, before);
    Ok(())
}
