#[path = "support/skills.rs"]
pub mod skill_support;
pub mod support;

use skill_support::{MANIFEST, configured_fixture, run};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn local_preserves_the_same_agent_and_scope_filters_as_default_doctor() -> TestResult {
    let fixture = configured_fixture()?;
    for filters in [
        vec![],
        vec!["--agent", "claude"],
        vec!["--scope", "user"],
        vec!["--scope", "project"],
        vec!["--agent", "cursor", "--scope", "project"],
    ] {
        let mut args = vec!["doctor", "skills", "--format", "json"];
        args.extend(filters);
        let expected = run(&fixture, &args)?;
        args.push("--local");
        let actual = run(&fixture, &args)?;
        assert_eq!(actual, expected, "{args:?}");
    }
    Ok(())
}

#[test]
fn installed_skills_ignore_generic_paths_but_keep_concrete_missing_references() -> TestResult {
    let fixture = configured_fixture()?;
    fixture.write_repository(
        "harness/skills/alpha/SKILL.md",
        "# Alpha\nscripts/ `assets/` evals/ `agents/` references/\nreferences/<topic>-<scope>.md `scripts/{name}.sh` assets/*\n./<skills-root>/<slug> `../.agents/skills` ../../outside/secret\n`[<text>](<slug>)`\n",
    )?;
    fixture.write_repository(
        "harness/skills/alpha/references/guide.md",
        "agents/openai.yaml `assets/icon.svg` evals/case.json `scripts/run.sh`\n",
    )?;
    for (contents, code, state) in [
        (None, 0, "healthy"),
        (Some("`references/github.md`\n"), 1, "drift"),
        (Some("[missing](references/absent.md)\n"), 1, "drift"),
    ] {
        if let Some(contents) = contents {
            fixture.write_repository("harness/skills/alpha/SKILL.md", contents)?;
        }
        let (actual, stdout, _) = run(
            &fixture,
            &["doctor", "skills", "--agent", "claude", "--format", "json"],
        )?;
        assert_eq!(actual, code, "{stdout}");
        let diagnostics: serde_json::Value = serde_json::from_str(&stdout)?;
        let diagnostics = diagnostics.as_array().ok_or("expected diagnostics")?;
        assert!(
            diagnostics.iter().any(|value| value["state"] == state),
            "{stdout}"
        );
        assert!(
            diagnostics
                .iter()
                .all(|value| value["state"] == "healthy" || value["state"] == state),
            "{stdout}"
        );
        let link_count = diagnostics
            .iter()
            .filter(|value| value["rule"] == "link")
            .count();
        assert_eq!(link_count, usize::from(code != 0), "{stdout}");
    }
    Ok(())
}

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
    Ok(())
}
