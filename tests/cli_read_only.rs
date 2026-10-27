#[path = "support/doctor_aggregate.rs"]
pub mod aggregate_support;
#[path = "../src/cli.rs"]
pub mod cli;
pub mod support;

#[path = "cli_read_only/git.rs"]
mod git;

use clap::{CommandFactory, ValueEnum};

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;
use support::Fixture;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Eq, PartialEq)]
struct Entry {
    modified: SystemTime,
    mode: u32,
    contents: Vec<u8>,
}

fn snapshot(directory: &Path, excluded: Option<&Path>) -> TestResult<BTreeMap<PathBuf, Entry>> {
    let mut entries = BTreeMap::new();
    collect(directory, excluded, &mut entries)?;
    Ok(entries)
}

fn collect(
    directory: &Path,
    excluded: Option<&Path>,
    entries: &mut BTreeMap<PathBuf, Entry>,
) -> TestResult {
    if excluded == Some(directory) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(directory)?;
    let contents = if metadata.is_symlink() {
        fs::read_link(directory)?
            .as_os_str()
            .as_encoded_bytes()
            .to_vec()
    } else if metadata.is_file() {
        Sha256::digest(fs::read(directory)?).to_vec()
    } else {
        Vec::new()
    };
    entries.insert(
        directory.to_owned(),
        Entry {
            modified: metadata.modified()?,
            mode: metadata.permissions().mode(),
            contents,
        },
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(directory)? {
            collect(&entry?.path(), excluded, entries)?;
        }
    }
    Ok(())
}

fn fixture() -> TestResult<Fixture> {
    let fixture = Fixture::new()?;
    let manifest = aggregate_support::MANIFEST
        .replace("scopes: [project]", "scopes: [user, project]")
        .replacen(
            "skills:",
            "  - id: cursor\n    scopes: [user, project]\nskills:",
            1,
        );
    fixture.write_home(".arnes.yaml", &manifest)?;
    for (path, contents) in [
        (
            ".claude/settings.json",
            "{\"hooks\":{},\"model\":\"example\"}",
        ),
        (".codex/config.toml", "model = 'example'\n"),
        (".codex/hooks.json", "{\"hooks\":{}}"),
        (".cursor/hooks.json", "{\"version\":1,\"hooks\":{}}"),
        (".cursor/settings.json", "{}"),
        (".local/bin/agent-measure", "#!/bin/sh\nexit 99\n"),
        ("skills/alpha/SKILL.md", "# Alpha\n"),
        ("instructions.md", "private instructions\n"),
        (
            ".claude/commands/deploy.md",
            "---\ndescription: Deploy safely\n---\nDeploy now\n",
        ),
    ] {
        fixture.write_home(path, contents)?;
    }
    fs::set_permissions(
        fixture.home().join(".local/bin/agent-measure"),
        fs::Permissions::from_mode(0o755),
    )?;
    for root in [".claude", ".codex", ".cursor", ".agents"] {
        let skills = fixture.home().join(root).join("skills");
        fs::create_dir_all(&skills)?;
        symlink(fixture.home().join("skills/alpha"), skills.join("alpha"))?;
        symlink(
            fixture.home().join("instructions.md"),
            fixture.home().join(root).join("AGENTS.md"),
        )?;
    }
    for (path, contents) in [
        ("harness/AGENTS.md", "global instructions\n"),
        ("harness/SOUL.md", "identity\n"),
        ("harness/USER.md", "preferences\n"),
        ("harness/skills/alpha/SKILL.md", "# Alpha\n"),
        ("home/.arnes.yaml", "version: 1\nhooks: []\n"),
        (
            "harness/prompts/deploy.md",
            "---\ndescription: Deploy safely\n---\nDeploy now\n",
        ),
        ("harness/rules/aggregate.md", "rule\n"),
        ("bin/mcp", "#!/bin/sh\nexit 99\n"),
        (
            ".mcp.json",
            "{\"mcpServers\":{\"managed\":{\"command\":\"bin/mcp\"}}}",
        ),
        (
            ".codex/config.toml",
            "[tui]\nstatus_line = [\"model\", \"current-dir\"]\n",
        ),
        ("export.json", "{\"harness/AGENTS.md\":\"instructions\"}"),
        (".gitignore", ".harness-export/\n"),
    ] {
        fixture.write_repository(path, contents)?;
    }
    fs::set_permissions(
        fixture.repository().join("bin/mcp"),
        fs::Permissions::from_mode(0o755),
    )?;
    fs::create_dir_all(fixture.home().join(".claude/rules"))?;
    symlink(
        fixture.repository().join("harness/rules/aggregate.md"),
        fixture.home().join(".claude/rules/aggregate.md"),
    )?;
    symlink(
        fixture.repository().join("harness/AGENTS.md"),
        fixture.home().join(".claude/CLAUDE.md"),
    )?;
    initialize_git(&fixture)?;
    fs::create_dir(fixture.repository().join(".harness-export"))?;
    Ok(fixture)
}

fn initialize_git(fixture: &Fixture) -> TestResult {
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(fixture.repository())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", fixture.home())
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn invoke(fixture: &Fixture, args: &[&str]) -> TestResult<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_arnes"))
        .args(args)
        .current_dir(fixture.repository())
        .env_clear()
        .env("HOME", fixture.home())
        .env("ARNES_SOURCE_ROOT", fixture.repository())
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", fixture.home().join("bin").display()),
        )
        .output()?)
}

fn unchanged(fixture: &Fixture, args: &[&str], export: bool) -> TestResult<Output> {
    let root = fixture.home().parent().ok_or("missing fixture root")?;
    let output_dir = fixture.repository().join(".harness-export");
    let excluded = export.then_some(output_dir.as_path());
    let before = snapshot(root, excluded)?;
    let output = invoke(fixture, args)?;
    let after = snapshot(root, excluded)?;
    let changed = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        changed.is_empty(),
        "filesystem changed for {args:?}: {changed:?}"
    );
    Ok(output)
}

#[test]
fn removed_mutators_are_rejected_without_writing() -> TestResult {
    let fixture = fixture()?;
    for agent in ["claude", "codex", "cursor"] {
        let output = unchanged(&fixture, &["setup", "hooks", "--agent", agent], false)?;
        assert_eq!(output.status.code(), Some(2));
        for resource in [
            "config",
            "instructions",
            "skills",
            "prompts",
            "commands",
            "rules",
            "mcp",
            "statusline",
        ] {
            let output = unchanged(
                &fixture,
                &["sync", resource, "--agent", agent, "--scope", "user"],
                false,
            )?;
            assert_eq!(output.status.code(), Some(2));
        }
    }
    Ok(())
}

#[test]
fn every_public_command_preserves_the_populated_home_and_source_tree() -> TestResult {
    let fixture = fixture()?;
    for subcommand in cli::Cli::command().get_subcommands() {
        let name = subcommand.get_name();
        unchanged(&fixture, &[name, "--help"], false)?;
        match name {
            "doctor" | "help" => {
                unchanged(&fixture, &[name], false)?;
            }
            "export" => {
                assert!(unchanged(&fixture, &[name], true)?.status.success());
            }
            _ => return Err(format!("add executable read-only coverage for {name}").into()),
        }
    }
    for args in [
        vec![],
        vec!["--help"],
        vec!["--version"],
        vec!["help"],
        vec!["help", "doctor"],
        vec!["help", "export"],
        vec!["doctor", "--help"],
        vec!["export", "--help"],
    ] {
        unchanged(&fixture, &args, false)?;
    }
    let resources = std::iter::once(String::new()).chain(
        cli::Resource::value_variants()
            .iter()
            .filter_map(ValueEnum::to_possible_value)
            .map(|value| value.get_name().to_owned()),
    );
    for resource in resources {
        for agent in ["claude", "codex", "cursor"] {
            for scope in ["user", "project"] {
                for format in ["human", "json"] {
                    let mut args = vec!["doctor"];
                    if !resource.is_empty() {
                        args.push(resource.as_str());
                    }
                    args.extend(["--agent", agent, "--scope", scope, "--format", format]);
                    if resource == "skills" {
                        args.push("--local");
                    }
                    let output = unchanged(&fixture, &args, false)?;
                    assert!(
                        output.stderr.is_empty(),
                        "{args:?}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
    }
    for args in [
        vec!["doctor", "--verbose", "--color", "always"],
        vec!["doctor", "skills", "--path", "harness/skills"],
        vec!["doctor", "skills", "--path", "missing"],
        vec!["doctor", "unknown"],
    ] {
        unchanged(&fixture, &args, false)?;
    }
    for selection in [None, Some("export.json")] {
        let mut args = vec!["export"];
        if let Some(selection) = selection {
            args.extend(["--sources", selection]);
        }
        for _ in 0..2 {
            let output = unchanged(&fixture, &args, true)?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let mut check = args.clone();
            check.push("--check");
            assert!(unchanged(&fixture, &check, false)?.status.success());
        }
    }
    fixture.write_repository("harness/AGENTS.md", "changed\n")?;
    assert_eq!(
        unchanged(
            &fixture,
            &["export", "--check", "--sources", "export.json"],
            false
        )?
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        unchanged(&fixture, &["export", "--sources", "missing.json"], false)?
            .status
            .code(),
        Some(2)
    );
    Ok(())
}

#[test]
fn export_never_executes_a_configured_git_fsmonitor() -> TestResult {
    let fixture = fixture()?;
    fixture.write_repository(
        "fsmonitor",
        "#!/bin/sh\nprintf changed > \"$HOME/.claude/settings.json\"\nprintf 'token\\000'\n",
    )?;
    let hook = fixture.repository().join("fsmonitor");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
    for value in [hook.as_os_str(), std::ffi::OsStr::new("true")] {
        let config = Command::new("git")
            .args(["config", "core.fsmonitor"])
            .arg(value)
            .current_dir(fixture.repository())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", fixture.home())
            .output()?;
        assert!(config.status.success());
        for selection in [None, Some("export.json")] {
            let mut args = vec!["export"];
            if let Some(selection) = selection {
                args.extend(["--sources", selection]);
            }
            let output = unchanged(&fixture, &args, true)?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            args.push("--check");
            assert!(unchanged(&fixture, &args, false)?.status.success());
        }
    }
    Ok(())
}

#[test]
fn first_export_only_adds_output_and_updates_its_parent_directory_entry() -> TestResult {
    let fixture = fixture()?;
    let output_dir = fixture.repository().join(".harness-export");
    fs::remove_dir(&output_dir)?;
    let root = fixture.home().parent().ok_or("missing fixture root")?;
    let mut before = snapshot(root, Some(&output_dir))?;
    let output = invoke(&fixture, &["export"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut after = snapshot(root, Some(&output_dir))?;
    let parent_before = before
        .remove(fixture.repository())
        .ok_or("missing parent before export")?;
    let parent_after = after
        .remove(fixture.repository())
        .ok_or("missing parent after export")?;
    assert_eq!(parent_before.mode, parent_after.mode);
    assert_eq!(parent_before.contents, parent_after.contents);
    assert_eq!(before, after);
    assert!(output_dir.join("00-MANIFEST.md").is_file());
    Ok(())
}
