use super::{Fixture, TestResult, fixture, snapshot};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn config(fixture: &Fixture, global: bool, key: &str, value: &str) -> TestResult {
    let mut command = Command::new("git");
    command.arg("config");
    if global {
        command.arg("--global");
    }
    let output = command
        .args([key, value])
        .current_dir(fixture.repository())
        .env_clear()
        .env("HOME", fixture.home())
        .env("PATH", "/usr/bin:/bin")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn assert_exports_preserve_files(
    fixture: &Fixture,
    environment: Option<(&str, &Path)>,
) -> TestResult {
    let root = fixture.home().parent().ok_or("missing fixture root")?;
    let export = fixture.repository().join(".harness-export");
    for selection in [None, Some("export.json")] {
        for check in [false, true] {
            let excluded = if check { None } else { Some(export.as_path()) };
            let before = snapshot(root, excluded)?;
            let mut command = Command::new(env!("CARGO_BIN_EXE_arnes"));
            command.arg("export");
            if let Some(selection) = selection {
                command.args(["--sources", selection]);
            }
            if check {
                command.arg("--check");
            }
            command
                .current_dir(fixture.repository())
                .env_clear()
                .env("HOME", fixture.home())
                .env("PATH", "/usr/bin:/bin");
            if let Some((name, value)) = environment {
                command.env(name, value);
            }
            let output = command.output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let after = snapshot(root, excluded)?;
            let changed = before
                .keys()
                .chain(after.keys())
                .filter(|path| before.get(*path) != after.get(*path))
                .collect::<std::collections::BTreeSet<_>>();
            assert!(
                changed.is_empty(),
                "export changed files with {environment:?}, check={check}, selection={selection:?}: {changed:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn inherited_git_trace_destinations_cannot_write_harness_files() -> TestResult {
    for name in [
        "GIT_TRACE",
        "GIT_TRACE_PERFORMANCE",
        "GIT_TRACE_SETUP",
        "GIT_TRACE_REFS",
        "GIT_TRACE2",
        "GIT_TRACE2_EVENT",
        "GIT_TRACE2_PERF",
    ] {
        let fixture = fixture()?;
        assert_exports_preserve_files(
            &fixture,
            Some((name, &fixture.home().join(".claude/settings.json"))),
        )?;
    }
    Ok(())
}

#[test]
fn configured_git_trace_destinations_cannot_write_harness_files() -> TestResult {
    for global in [false, true] {
        for key in [
            "trace2.normalTarget",
            "trace2.eventTarget",
            "trace2.perfTarget",
        ] {
            let fixture = fixture()?;
            config(
                &fixture,
                global,
                key,
                fixture
                    .home()
                    .join(".claude/settings.json")
                    .to_str()
                    .ok_or("non-UTF-8 fixture")?,
            )?;
            assert_exports_preserve_files(&fixture, None)?;
        }
    }
    Ok(())
}

#[test]
fn inspecting_a_modified_tracked_source_never_executes_git_filters() -> TestResult {
    let fixture = fixture()?;
    fixture.write_repository(
        "filter",
        "#!/bin/sh\nprintf changed > \"$HOME/.claude/settings.json\"\ncat\n",
    )?;
    let filter = fixture.repository().join("filter");
    fs::set_permissions(&filter, fs::Permissions::from_mode(0o755))?;
    config(
        &fixture,
        false,
        "filter.sideeffect.clean",
        filter.to_str().ok_or("non-UTF-8 fixture")?,
    )?;
    fixture.write_repository(".gitattributes", "harness/AGENTS.md filter=sideeffect\n")?;
    fixture.write_repository("harness/AGENTS.md", "Global instructions\n")?;
    assert_exports_preserve_files(&fixture, None)?;
    Ok(())
}
