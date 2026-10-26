use crate::diagnostic::{Diagnostic, State};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::OnceLock;

static TOKENIZER: OnceLock<Result<tiktoken_rs::CoreBPE, String>> = OnceLock::new();

pub(super) fn diagnose(skill: &Path, local: bool) -> Vec<Diagnostic> {
    let file = skill.join("SKILL.md");
    let Ok(contents) = super::references::read_skill(skill, &file, "skill") else {
        return Vec::new();
    };
    let (body, line) = match body(&contents) {
        Ok(body) => body,
        Err(reason) => return vec![finding(&file, 1, "frontmatter", reason, State::Error)],
    };
    let mut diagnostics = vec![budget(&file, body, line)];
    diagnostics.extend(requires(&file, &contents, line, local));
    diagnostics
}

fn requires(file: &Path, contents: &str, body_line: usize, local: bool) -> Vec<Diagnostic> {
    if body_line == 1 {
        return Vec::new();
    }
    let yaml = contents
        .lines()
        .skip(1)
        .take(body_line - 3)
        .collect::<Vec<_>>()
        .join("\n");
    let document: serde_yaml_ng::Value = match serde_yaml_ng::from_str(&yaml) {
        Ok(document) => document,
        Err(error) => {
            return vec![finding(
                file,
                error.location().map_or(2, |location| location.line() + 1),
                "frontmatter",
                error.to_string(),
                State::Error,
            )];
        }
    };
    let Some(metadata) = document.get("metadata") else {
        return Vec::new();
    };
    let line = metadata_field_line(contents, body_line, "requires");
    let Some(metadata) = metadata.as_mapping().filter(|mapping| {
        mapping
            .iter()
            .all(|(key, value)| key.as_str().is_some() && value.as_str().is_some())
    }) else {
        return vec![finding(
            file,
            line,
            "requires",
            "metadata must contain string keys and string values",
            State::Error,
        )];
    };
    let Some(value) = metadata
        .get("requires")
        .and_then(serde_yaml_ng::Value::as_str)
    else {
        return Vec::new();
    };
    let names = value.split_whitespace().collect::<Vec<_>>();
    if names.is_empty()
        || names.iter().any(|name| {
            !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.+".contains(&byte))
                || matches!(*name, "." | "..")
        })
    {
        return vec![finding(
            file,
            line,
            "requires",
            "metadata.requires must be space-separated binary names",
            State::Error,
        )];
    }
    if !local {
        return Vec::new();
    }
    names
        .into_iter()
        .filter_map(|name| match binary_available(name) {
            Ok(true) => None,
            Ok(false) => Some(finding(
                file,
                line,
                "requires",
                format!("{name} is missing from PATH"),
                State::Drift,
            )),
            Err(reason) => Some(finding(
                file,
                line,
                "requires",
                format!("{name}: {reason}"),
                State::Error,
            )),
        })
        .collect()
}

fn metadata_field_line(contents: &str, body_line: usize, field: &str) -> usize {
    let keys = contents
        .lines()
        .take(body_line - 1)
        .enumerate()
        .filter_map(|(index, line)| {
            let (key, _) = line.split_once(':')?;
            let key: serde_yaml_ng::Value = serde_yaml_ng::from_str(key.trim()).ok()?;
            Some((
                index + 1,
                line.len() - line.trim_start().len(),
                key.as_str()?.to_owned(),
            ))
        })
        .collect::<Vec<_>>();
    let Some((line, indent, _)) = keys
        .iter()
        .filter(|(_, _, key)| key == "metadata")
        .min_by_key(|(_, indent, _)| indent)
    else {
        return 2;
    };
    keys.iter()
        .skip_while(|(number, _, _)| number <= line)
        .take_while(|(_, level, _)| level > indent)
        .filter(|(_, _, key)| key == field)
        .min_by_key(|(_, level, _)| level)
        .map_or(*line, |(number, _, _)| *number)
}

fn binary_available(name: &str) -> Result<bool, &'static str> {
    let Some(path) = std::env::var_os("PATH") else {
        return Ok(false);
    };
    for directory in std::env::split_paths(&path) {
        match fs::metadata(directory.join(name)) {
            Ok(metadata) if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 => {
                return Ok(true);
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::NotADirectory
                        | std::io::ErrorKind::PermissionDenied
                ) => {}
            Err(_) => return Err("PATH entry could not be read"),
        }
    }
    Ok(false)
}

fn body(contents: &str) -> Result<(&str, usize), &'static str> {
    let mut lines = contents.split_inclusive('\n');
    let Some(first) = lines.next().filter(|line| line.trim_end() == "---") else {
        return Ok((contents, 1));
    };
    let mut offset = first.len();
    for (index, line) in lines.enumerate() {
        offset += line.len();
        if line.trim_end() == "---" {
            return Ok((contents.get(offset..).unwrap_or_default(), index + 3));
        }
    }
    Err("frontmatter closing delimiter is missing")
}

fn budget(file: &Path, body: &str, line: usize) -> Diagnostic {
    let tokenizer =
        TOKENIZER.get_or_init(|| tiktoken_rs::o200k_base().map_err(|error| error.to_string()));
    let tokenizer = match tokenizer {
        Ok(tokenizer) => tokenizer,
        Err(reason) => return finding(file, line, "budget", reason, State::Error),
    };
    let tokens = tokenizer.encode_ordinary(body).len();
    let state = match tokens {
        0..4000 => State::Healthy,
        4000..=5000 => State::Warning,
        5001.. => State::Error,
    };
    let measurement = format!("{tokens}/5000");
    Diagnostic::new(
        "skills",
        state,
        format!("{}:{line} budget {measurement} o200k_base", file.display()),
    )
    .with_finding(file, line, "budget", measurement, Some("o200k_base"))
}

pub(super) fn finding(
    file: &Path,
    line: usize,
    rule: &'static str,
    measurement: impl Into<String>,
    state: State,
) -> Diagnostic {
    let measurement = measurement.into();
    Diagnostic::new("skills", state, &measurement).with_finding(file, line, rule, measurement, None)
}

pub(super) fn dated_files(skill: &Path, subject: &str) -> Vec<Diagnostic> {
    let (files, _) = super::references::files(skill, subject);
    let references = skill.join("references");
    files
        .into_iter()
        .filter(|file| {
            file.starts_with(&references)
                && file.file_name().is_some_and(|name| {
                    name.as_encoded_bytes().windows(10).any(|part| {
                        matches!(
                            part,
                            [
                                b'2',
                                b'0',
                                b'0'..=b'9',
                                b'0'..=b'9',
                                b'-',
                                b'0'..=b'1',
                                b'0'..=b'9',
                                b'-',
                                b'0'..=b'3',
                                b'0'..=b'9'
                            ]
                        )
                    })
                })
        })
        .map(|file| {
            finding(
                &file,
                1,
                "dated-file",
                "dated filename under references",
                State::Error,
            )
        })
        .collect()
}
