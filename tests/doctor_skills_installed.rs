#[path = "support/skills.rs"]
pub mod skill_support;
pub mod support;

use skill_support::{MANIFEST, configured_fixture, run};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn installed_skills_apply_budget_and_requires_but_allow_dated_files() -> TestResult {
    let fixture = configured_fixture()?;
    fixture.write_repository(
        "harness/skills/alpha/references/evidence-2026-10-26.txt",
        "evidence",
    )?;
    for (body, arguments, code, expected) in [
        (" a".repeat(4050), Vec::new(), 0, "warning"),
        (" a".repeat(5050), Vec::new(), 2, "error"),
        (
            "---\nmetadata:\n  requires: [missing]\n---\nbody".to_owned(),
            Vec::new(),
            2,
            "requires",
        ),
        (
            "---\nmetadata:\n  requires: arnes-missing-binary\n---\nbody".to_owned(),
            Vec::new(),
            0,
            "healthy",
        ),
        (
            "---\nmetadata:\n  requires: arnes-missing-binary\n---\nbody".to_owned(),
            vec!["--local"],
            1,
            "requires",
        ),
    ] {
        fixture.write_repository("harness/skills/alpha/SKILL.md", &body)?;
        let mut args = vec!["doctor", "skills", "--agent", "claude", "--format", "json"];
        args.extend(arguments);
        let (actual, stdout, _) = run(&fixture, &args)?;
        assert_eq!(actual, code, "{stdout}");
        assert!(stdout.contains(expected), "{stdout}");
        assert!(!stdout.contains("dated-file"), "{stdout}");
    }
    Ok(())
}

#[test]
fn nested_links_to_declared_skills_are_allowed_only_in_installed_user_projection() -> TestResult {
    let fixture = configured_fixture()?;
    fixture.write_home(
        ".arnes.yaml",
        &MANIFEST.replace(
            "resources:",
            "  - slug: beta\n    installations:\n      - { agent: codex, scope: user }\nresources:",
        ),
    )?;
    fixture.write_repository("harness/skills/beta/SKILL.md", "# Beta\n")?;
    skill_support::link_home(&fixture, "harness/skills/beta", ".agents/skills/beta")?;
    fixture.write_repository(
        "harness/skills/alpha/references/guide.md",
        "# Guide\n[workflow](../../beta/SKILL.md)\n",
    )?;
    let (code, stdout, _) = run(&fixture, &["doctor", "skills", "--format", "json"])?;
    assert_eq!(code, 0, "{stdout}");
    let (code, stdout, _) = run(
        &fixture,
        &[
            "doctor",
            "skills",
            "--path",
            "harness/skills",
            "--format",
            "json",
        ],
    )?;
    assert_eq!(code, 2, "{stdout}");
    assert!(stdout.contains("references/guide.md"), "{stdout}");
    fixture.write_repository(".agents/skills/beta/SKILL.md", "# Beta\n")?;
    fixture.write_repository(
        ".agents/skills/project-alpha/references/guide.md",
        "[workflow](../../beta/SKILL.md)\n",
    )?;
    let (code, stdout, _) = run(
        &fixture,
        &[
            "doctor", "skills", "--agent", "claude", "--scope", "project", "--format", "json",
        ],
    )?;
    assert_eq!(code, 2, "{stdout}");
    assert!(stdout.contains("escapes its skill"), "{stdout}");
    let output = fixture.command(["sync", "skills", "--agent", "claude", "--scope", "user"])?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8(output.stdout)?.contains("escapes its skill"));
    Ok(())
}
