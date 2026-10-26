use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn run(directory: &TempDir, extra: &[&str]) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_arnes"))
        .env_clear()
        .current_dir(directory.path())
        .args(["doctor", "skills", "--path", "skills"])
        .args(extra)
        .output()
}

#[test]
fn repository_mode_needs_neither_home_nor_manifest() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    fs::create_dir_all(directory.path().join("skills/ignored"))?;
    fs::write(directory.path().join("skills/alpha/SKILL.md"), "# Alpha\n")?;
    let output = run(&directory, &["--format", "json"])?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let findings = diagnostics.as_array().ok_or("expected diagnostic array")?;
    assert_eq!(findings.len(), 1);
    let budget = findings.first().ok_or("missing budget")?;
    assert_eq!(budget["file"], "skills/alpha/SKILL.md");
    assert_eq!(budget["line"], 1);
    assert_eq!(budget["rule"], "budget");
    assert_eq!(budget["tokenizer"], "o200k_base");
    assert_eq!(budget["state"], "healthy");
    Ok(())
}

#[test]
fn exact_body_budget_reports_thresholds_and_excludes_frontmatter() -> TestResult {
    for (tokens, state, code) in [
        (3950, "healthy", 0),
        (4000, "warning", 0),
        (4050, "warning", 0),
        (5000, "warning", 0),
        (5050, "error", 2),
    ] {
        let directory = tempfile::tempdir()?;
        fs::create_dir_all(directory.path().join("skills/alpha"))?;
        fs::write(
            directory.path().join("skills/alpha/SKILL.md"),
            format!(
                "---\ndescription: {}\n---\n{}",
                "large ".repeat(6000),
                " a".repeat(tokens)
            ),
        )?;
        let output = run(&directory, &["--format", "json"])?;
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let budget = diagnostics
            .as_array()
            .ok_or("expected diagnostics")?
            .iter()
            .find(|value| value["rule"] == "budget")
            .ok_or("missing budget")?;
        assert_eq!(budget["measurement"], format!("{tokens}/5000"));
        assert_eq!(budget["state"], state);
        assert_eq!(budget["line"], 4);
        assert_eq!(budget["tokenizer"], "o200k_base");
        let human = run(&directory, &["--verbose"])?;
        let stdout = String::from_utf8(human.stdout)?;
        assert!(
            stdout.contains(&format!(
                "skills/alpha/SKILL.md:4 budget {tokens}/5000 o200k_base"
            )),
            "{stdout}"
        );
        assert!(stdout.to_lowercase().contains(state), "{stdout}");
    }
    Ok(())
}

#[test]
fn every_markdown_file_reports_each_link_at_its_source_line() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha/references"))?;
    fs::write(directory.path().join("skills/alpha/SKILL.md"), "# Alpha\n")?;
    fs::write(
        directory.path().join("skills/alpha/references/x.md"),
        "# Guide\n```\n[ignored](missing.md)\n```\n\n[missing](missing.md)\n[outside](../../outside.md)\n",
    )?;
    fs::write(directory.path().join("skills/outside.md"), "outside")?;
    let output = run(&directory, &["--format", "json"])?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let links = diagnostics
        .as_array()
        .ok_or("expected diagnostics")?
        .iter()
        .filter(|value| value["rule"] == "link")
        .collect::<Vec<_>>();
    assert_eq!(links.len(), 2, "{diagnostics}");
    assert!(
        links
            .iter()
            .any(|link| link["state"] == "drift" && link["line"] == 6)
    );
    assert!(
        links
            .iter()
            .any(|link| link["state"] == "error" && link["line"] == 7)
    );
    assert!(
        links
            .iter()
            .all(|link| link["file"] == "skills/alpha/references/x.md")
    );
    Ok(())
}

#[test]
fn requires_validates_shape_and_only_checks_binaries_with_local() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    let file = directory.path().join("skills/alpha/SKILL.md");
    for metadata in [
        "requires: [missing]",
        "requires: 12",
        "requires: null",
        "requires: ''",
        "requires: '../missing'",
        "requires: 'a;bad'",
        "requires: 'tool'\n  other: []",
    ] {
        fs::write(&file, format!("---\nmetadata:\n  {metadata}\n---\nbody"))?;
        let output = run(&directory, &["--format", "json"])?;
        assert_eq!(output.status.code(), Some(2), "{metadata}: {output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert!(
            diagnostics
                .as_array()
                .ok_or("expected diagnostics")?
                .iter()
                .any(|value| value["rule"] == "requires" && value["line"] == 3),
            "{diagnostics}"
        );
    }
    fs::write(
        file,
        "---\nmetadata:\n  requires: arnes-missing-one arnes-missing-two\n---\nbody",
    )?;
    assert_eq!(run(&directory, &[])?.status.code(), Some(0));
    let output = run(&directory, &["--local", "--format", "json"])?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let missing = diagnostics
        .as_array()
        .ok_or("expected diagnostics")?
        .iter()
        .filter(|value| value["rule"] == "requires")
        .collect::<Vec<_>>();
    assert_eq!(missing.len(), 2);
    assert!(missing.iter().all(|value| value["state"] == "drift"));
    Ok(())
}

#[test]
fn dated_files_are_rejected_only_below_references() -> TestResult {
    let directory = tempfile::tempdir()?;
    for parent in ["references/nested", "evals"] {
        fs::create_dir_all(directory.path().join("skills/alpha").join(parent))?;
    }
    fs::write(directory.path().join("skills/alpha/SKILL.md"), "# Alpha\n")?;
    fs::write(
        directory
            .path()
            .join("skills/alpha/references/nested/evidence-2026-19-39.txt"),
        "evidence",
    )?;
    fs::write(
        directory
            .path()
            .join("skills/alpha/references/not-1999-01-01.md"),
        "guide",
    )?;
    fs::write(
        directory
            .path()
            .join("skills/alpha/evals/evidence-2026-10-26.md"),
        "evidence",
    )?;
    let output = run(&directory, &["--format", "json"])?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let dated = diagnostics
        .as_array()
        .ok_or("expected diagnostics")?
        .iter()
        .filter(|value| value["rule"] == "dated-file")
        .collect::<Vec<_>>();
    assert_eq!(dated.len(), 1, "{diagnostics}");
    assert_eq!(
        dated.first().ok_or("missing dated file")?["file"],
        "skills/alpha/references/nested/evidence-2026-19-39.txt"
    );
    Ok(())
}

#[test]
fn main_file_links_distinguish_missing_targets_from_escapes() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    fs::write(directory.path().join("skills/outside.md"), "outside")?;
    for (reference, code, state) in [
        ("missing.md", 1, "drift"),
        ("../outside.md", 2, "error"),
        ("../absent.md", 2, "error"),
    ] {
        fs::write(
            directory.path().join("skills/alpha/SKILL.md"),
            format!("# Alpha\n[link]({reference})\n"),
        )?;
        let output = run(&directory, &["--format", "json"])?;
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let link = diagnostics
            .as_array()
            .ok_or("expected diagnostics")?
            .iter()
            .find(|value| value["rule"] == "link")
            .ok_or("missing link diagnostic")?;
        assert_eq!(link["state"], state);
        assert_eq!(link["file"], "skills/alpha/SKILL.md");
        assert_eq!(link["line"], 2);
        let human = run(&directory, &[])?;
        assert!(String::from_utf8(human.stdout)?.contains("skills/alpha/SKILL.md:2 link"));
    }
    Ok(())
}

#[test]
fn local_search_accepts_executables_without_running_them() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    fs::create_dir(directory.path().join("bin"))?;
    fs::write(
        directory.path().join("skills/alpha/SKILL.md"),
        "---\nmetadata:\n  requires: tool\n---\nbody",
    )?;
    let binary = directory.path().join("bin/tool");
    fs::write(&binary, "invalid executable contents")?;
    for (mode, code) in [(0o644, 1), (0o755, 0)] {
        fs::set_permissions(&binary, fs::Permissions::from_mode(mode))?;
        let output = Command::new(env!("CARGO_BIN_EXE_arnes"))
            .env_clear()
            .env("PATH", directory.path().join("bin"))
            .current_dir(directory.path())
            .args(["doctor", "skills", "--path", "skills", "--local"])
            .output()?;
        assert_eq!(output.status.code(), Some(code), "{output:?}");
    }
    Ok(())
}

#[test]
fn unreadable_and_malformed_inputs_fail_instead_of_appearing_healthy() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    for contents in [
        b"\xff".as_slice(),
        b"---\nmetadata: [\n",
        b"---\nmetadata: [\n---\nbody",
    ] {
        fs::write(directory.path().join("skills/alpha/SKILL.md"), contents)?;
        let output = run(&directory, &["--format", "json"])?;
        assert_eq!(output.status.code(), Some(2), "{output:?}");
    }
    let missing = tempfile::tempdir()?;
    assert_eq!(run(&missing, &[])?.status.code(), Some(2));
    Ok(())
}

#[test]
fn markdown_symlinks_stay_confined_and_directory_cycles_terminate() -> TestResult {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha/references"))?;
    fs::write(
        directory.path().join("skills/alpha/SKILL.md"),
        "The imaginary-skill is cited without a semantic check.\n",
    )?;
    symlink("..", directory.path().join("skills/alpha/references/cycle"))?;
    assert_eq!(run(&directory, &[])?.status.code(), Some(0));
    fs::write(directory.path().join("outside.md"), "outside")?;
    symlink(
        "../../../outside.md",
        directory.path().join("skills/alpha/references/x.md"),
    )?;
    let output = run(&directory, &["--format", "json"])?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert!(
        diagnostics
            .as_array()
            .ok_or("expected diagnostics")?
            .iter()
            .any(|value| value["file"] == "skills/alpha/references/x.md"
                && value["state"] == "error")
    );
    Ok(())
}

#[test]
fn crlf_frontmatter_does_not_contribute_a_token_to_the_body() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    fs::write(
        directory.path().join("skills/alpha/SKILL.md"),
        format!(
            "---\r\nmetadata:\r\n  requires: tool\r\n---\r\n{}",
            " a".repeat(5000)
        ),
    )?;
    let output = run(&directory, &["--format", "json"])?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert!(
        diagnostics
            .as_array()
            .ok_or("expected diagnostics")?
            .iter()
            .any(|value| value["rule"] == "budget"
                && value["measurement"] == "5000/5000"
                && value["line"] == 5)
    );
    Ok(())
}

#[test]
fn requires_locations_follow_the_metadata_mapping_and_quoted_keys() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    for contents in [
        "---\ndescription: |\n  requires: text only\nmetadata:\n  requires: [bad]\n---\nbody",
        "---\nname: alpha\ndescription: alpha\nmetadata:\n  \"requires\": [bad]\n---\nbody",
    ] {
        fs::write(directory.path().join("skills/alpha/SKILL.md"), contents)?;
        let output = run(&directory, &["--format", "json"])?;
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let requires = diagnostics
            .as_array()
            .ok_or("expected diagnostics")?
            .iter()
            .find(|value| value["rule"] == "requires")
            .ok_or("missing requires")?;
        assert_eq!(requires["line"], 5, "{diagnostics}");
    }
    Ok(())
}

#[test]
fn local_search_continues_past_path_entries_that_are_not_directories() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    fs::create_dir(directory.path().join("bin"))?;
    fs::write(directory.path().join("not-a-directory"), "file")?;
    fs::write(directory.path().join("bin/tool"), "unused")?;
    fs::set_permissions(
        directory.path().join("bin/tool"),
        fs::Permissions::from_mode(0o755),
    )?;
    fs::write(
        directory.path().join("skills/alpha/SKILL.md"),
        "---\nmetadata:\n  requires: tool\n---\nbody",
    )?;
    let path = std::env::join_paths([
        directory.path().join("not-a-directory"),
        directory.path().join("bin"),
    ])?;
    let output = Command::new(env!("CARGO_BIN_EXE_arnes"))
        .env_clear()
        .env("PATH", path)
        .current_dir(directory.path())
        .args(["doctor", "skills", "--path", "skills", "--local"])
        .output()?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    Ok(())
}

#[test]
fn symlink_then_parent_uses_the_actual_target_instead_of_a_lexical_decoy() -> TestResult {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha/references"))?;
    fs::write(directory.path().join("skills/alpha/SKILL.md"), "# Alpha\n")?;
    fs::write(
        directory.path().join("skills/alpha/references/secret.md"),
        "internal decoy",
    )?;
    fs::write(directory.path().join("skills/secret.md"), "external target")?;
    symlink("..", directory.path().join("skills/alpha/references/jump"))?;
    for target in ["secret.md", "absent.md"] {
        fs::write(
            directory.path().join("skills/alpha/references/x.md"),
            format!("[link](jump/../{target})\n"),
        )?;
        let output = run(&directory, &["--format", "json"])?;
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert!(
            diagnostics
                .as_array()
                .ok_or("expected diagnostics")?
                .iter()
                .any(|value| value["file"] == "skills/alpha/references/x.md"
                    && value["line"] == 1
                    && value["state"] == "error")
        );
    }
    Ok(())
}

#[test]
fn non_regular_skill_files_are_rejected_without_reading_them() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("skills/alpha"))?;
    assert!(
        Command::new("mkfifo")
            .env_clear()
            .arg(directory.path().join("skills/alpha/SKILL.md"))
            .status()?
            .success()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_arnes"))
        .env_clear()
        .current_dir(directory.path())
        .args(["doctor", "skills", "--path", "skills", "--format", "json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while child.try_wait()?.is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if child.try_wait()?.is_none() {
        child.kill()?;
    }
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8(output.stdout)?.contains("is not a file"));
    Ok(())
}
