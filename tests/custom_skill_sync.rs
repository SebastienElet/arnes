#![cfg(test)]
pub mod support;

use std::fs;
use std::os::unix::fs::symlink;
use support::Fixture;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const MANIFEST: &str = "version: 1
agents:
  - { id: claude, scopes: [user, project] }
skills:
  - slug: alpha
    source: CUSTOM_SOURCE
    installations: [{ agent: claude, scope: user }]
resources:
  - id: user-skills
    kind: skills
    agent: claude
    scope: user
    layout: leaves
    source: { root: repository, path: harness/skills }
    destination: { root: home, path: .claude/skills }
  - id: project-instructions
    kind: instructions
    agent: claude
    scope: project
    source: { root: repository, path: AGENTS.md }
    destination: { root: repository, path: CLAUDE.md }
";

fn fixture(source: &str) -> Result<Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = Fixture::new()?;
    fixture.write_home(".arnes.yaml", &MANIFEST.replace("CUSTOM_SOURCE", source))?;
    fixture.write_repository("AGENTS.md", "# Project policy\n")?;
    Ok(fixture)
}

#[test]
fn custom_source_links_can_resolve_outside_their_parent_within_the_source_root() -> TestResult {
    let fixture = fixture("vendor/skills/alias")?;
    fixture.write_repository("vendor/other/custom/SKILL.md", "# Custom\n")?;
    fs::create_dir_all(fixture.repository().join("vendor/skills"))?;
    symlink(
        "../other/custom",
        fixture.repository().join("vendor/skills/alias"),
    )?;
    let destination = fixture.home().join(".claude/skills/alpha");
    fs::create_dir_all(destination.parent().ok_or("parent")?)?;
    symlink(
        fixture.repository().join("vendor/skills/alias"),
        &destination,
    )?;
    let before = fixture.snapshot()?;
    let output = fixture.command(["doctor", "skills", "--agent", "claude", "--scope", "user"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fixture.snapshot()?, before);
    fs::remove_file(&destination)?;
    let output = fixture.command(["sync", "skills", "--agent", "claude", "--scope", "user"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::canonicalize(&destination)?,
        fs::canonicalize(fixture.repository().join("vendor/other/custom"))?
    );
    let before = fixture.snapshot()?;
    let output = fixture.command(["sync", "skills", "--agent", "claude", "--scope", "user"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fixture.snapshot()?, before);
    Ok(())
}

#[test]
fn top_level_custom_skills_do_not_block_unrelated_project_publication() -> TestResult {
    for source in ["custom-skill", "vendor/custom-skill"] {
        let fixture = fixture(source)?;
        fixture.write_repository("custom-skill/SKILL.md", "# Custom\n")?;
        if source.starts_with("vendor/") {
            symlink(".", fixture.repository().join("vendor"))?;
        }
        let output = fixture.command([
            "sync",
            "instructions",
            "--agent",
            "claude",
            "--scope",
            "project",
        ])?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            fs::read(fixture.repository().join("CLAUDE.md"))?,
            b"@AGENTS.md\n"
        );
        assert_eq!(
            fs::read(fixture.repository().join("custom-skill/SKILL.md"))?,
            b"# Custom\n"
        );
    }
    Ok(())
}

#[test]
fn custom_skill_reservations_protect_top_level_and_aliased_targets() -> TestResult {
    for source in [".claude", "vendor/skills/alias"] {
        let fixture = fixture(source)?;
        fixture.write_repository(".claude/SKILL.md", "# Custom\n")?;
        if source != ".claude" {
            fs::create_dir_all(fixture.repository().join("vendor/skills"))?;
            symlink("../../.claude", fixture.repository().join(source))?;
        }
        fixture.write_repository("prompt.md", "# Review\n")?;
        let manifest = format!(
            "{}\n{}",
            MANIFEST.replace("CUSTOM_SOURCE", source),
            r"prompts:
  - id: review
    source: { root: repository, path: prompt.md }
    includes: []
    variables: []
    projections:
      - agent: claude
        scope: project
        representation: file
        destination: { root: repository, path: .claude/commands/review.md }
"
        );
        fixture.write_home(".arnes.yaml", &manifest)?;
        let before = fixture.snapshot()?;
        let output =
            fixture.command(["sync", "prompts", "--agent", "claude", "--scope", "project"])?;
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("aliases a canonical source"),
            "{source}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(fixture.snapshot()?, before);
    }
    Ok(())
}

#[test]
fn custom_source_links_cannot_alias_the_project_skill_collection() -> TestResult {
    let fixture = fixture("vendor/skills/alias")?;
    fixture.write_repository(".agents/skills/project/SKILL.md", "# Project\n")?;
    fs::create_dir_all(fixture.repository().join("vendor/skills"))?;
    symlink(
        "../../.agents/skills/project",
        fixture.repository().join("vendor/skills/alias"),
    )?;
    let before = fixture.snapshot()?;
    let output = fixture.command(["sync", "skills", "--agent", "claude", "--scope", "user"])?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fixture.snapshot()?, before);
    Ok(())
}
