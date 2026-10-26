use crate::diagnostic::{Diagnostic, State};
use std::fs;
use std::path::{Component, Path, PathBuf};

mod parser;

use parser::local_references;

pub fn validate(skill: &Path, subject: &str) -> Result<(), Diagnostic> {
    validate_declared(skill, subject, &[])
}

pub fn validate_declared(
    skill: &Path,
    subject: &str,
    declared: &[PathBuf],
) -> Result<(), Diagnostic> {
    diagnose_declared(skill, subject, declared)
        .into_iter()
        .next()
        .map_or(Ok(()), Err)
}

pub fn diagnose_declared(skill: &Path, subject: &str, declared: &[PathBuf]) -> Vec<Diagnostic> {
    let main = skill.join("SKILL.md");
    if let Err(diagnostic) = read_skill(skill, &main, subject) {
        let measurement = diagnostic.message.clone();
        return vec![diagnostic.with_finding(&main, 1, "link", measurement, None)];
    }
    let (files, mut diagnostics) = files(skill, subject);
    for file in files
        .into_iter()
        .filter(|file| file.extension().is_some_and(|extension| extension == "md"))
    {
        let contents = match read_skill(skill, &file, subject) {
            Ok(contents) => contents,
            Err(diagnostic) => {
                let measurement = diagnostic.message.clone();
                diagnostics.push(diagnostic.with_finding(&file, 1, "link", measurement, None));
                continue;
            }
        };
        let base = file.parent().unwrap_or(skill);
        for (line, reference) in local_references(&contents) {
            if let Err(diagnostic) = validate_reference(skill, base, &reference, subject, declared)
            {
                let measurement = diagnostic.message.clone();
                diagnostics.push(diagnostic.with_finding(&file, line, "link", measurement, None));
            }
        }
    }
    diagnostics
}

pub(super) fn files(skill: &Path, subject: &str) -> (Vec<PathBuf>, Vec<Diagnostic>) {
    let mut pending = vec![(skill.to_path_buf(), Vec::new())];
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    while let Some((directory, mut ancestors)) = pending.pop() {
        let entries = ensure_within(&directory, skill, subject).and_then(|()| {
            fs::read_dir(&directory)
                .map_err(|_| broken(subject, State::Error, "directory could not be read"))
        });
        let entries = match entries {
            Ok(entries) => entries,
            Err(diagnostic) => {
                let measurement = diagnostic.message.clone();
                diagnostics.push(diagnostic.with_finding(&directory, 1, "link", measurement, None));
                continue;
            }
        };
        let resolved = fs::canonicalize(&directory).unwrap_or_else(|_| directory.clone());
        if ancestors.contains(&resolved) {
            continue;
        }
        ancestors.push(resolved);
        for entry in entries {
            let Ok(entry) = entry else {
                diagnostics.push(super::checks::finding(
                    &directory,
                    1,
                    "link",
                    "directory entry could not be read",
                    State::Error,
                ));
                continue;
            };
            let path = entry.path();
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_dir() => pending.push((path, ancestors.clone())),
                Ok(_) => files.push(path),
                Err(_) => diagnostics.push(super::checks::finding(
                    &path,
                    1,
                    "link",
                    "file metadata could not be read",
                    State::Error,
                )),
            }
        }
    }
    files.sort();
    (files, diagnostics)
}

pub(super) fn read_skill(skill: &Path, file: &Path, subject: &str) -> Result<String, Diagnostic> {
    let metadata = fs::metadata(file).map_err(|error| {
        let (state, reason) = if error.kind() == std::io::ErrorKind::NotFound {
            (State::Drift, "is missing")
        } else {
            (State::Error, "could not be read")
        };
        broken(subject, state, &format!("{} {reason}", file.display()))
    })?;
    if !metadata.is_file() {
        return Err(broken(
            subject,
            State::Error,
            &format!("{} is not a file", file.display()),
        ));
    }
    ensure_within(file, skill, subject)?;
    fs::read_to_string(file).map_err(|_| {
        broken(
            subject,
            State::Error,
            &format!("{} could not be read", file.display()),
        )
    })
}

fn validate_reference(
    skill: &Path,
    base: &Path,
    reference: &Path,
    subject: &str,
    declared: &[PathBuf],
) -> Result<(), Diagnostic> {
    let root = fs::canonicalize(skill)
        .map_err(|_| broken(subject, State::Error, "skill could not be resolved"))?;
    let base = fs::canonicalize(base).map_err(|_| {
        broken(
            subject,
            State::Error,
            "reference directory could not be resolved",
        )
    })?;
    let path = resolve(&root, &base, reference)
        .or_else(|| (!declared.is_empty()).then(|| base.join(reference)))
        .ok_or_else(|| {
            broken(
                subject,
                State::Error,
                &format!("local resource {} escapes its skill", reference.display()),
            )
        })?;
    if let Err(error) = fs::metadata(&path) {
        let missing = error.kind() == std::io::ErrorKind::NotFound;
        if missing {
            let mut ancestor = path.as_path();
            while fs::symlink_metadata(ancestor)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            {
                ancestor = ancestor.parent().ok_or_else(|| {
                    broken(
                        subject,
                        State::Error,
                        "reference parent could not be resolved",
                    )
                })?;
            }
            ensure_allowed(ancestor, skill, subject, declared)?;
        }
        return Err(broken(
            subject,
            if missing { State::Drift } else { State::Error },
            &format!(
                "local resource {} {}",
                reference.display(),
                if missing {
                    "is missing"
                } else {
                    "could not be read"
                }
            ),
        ));
    }
    ensure_allowed(&path, skill, subject, declared)
}

fn ensure_allowed(
    path: &Path,
    skill: &Path,
    subject: &str,
    declared: &[PathBuf],
) -> Result<(), Diagnostic> {
    ensure_within(path, skill, subject).or_else(|diagnostic| {
        if declared
            .iter()
            .any(|root| ensure_within(path, root, subject).is_ok())
        {
            Ok(())
        } else {
            Err(diagnostic)
        }
    })
}

fn ensure_within(path: &Path, skill: &Path, subject: &str) -> Result<(), Diagnostic> {
    let root = fs::canonicalize(skill).map_err(|_| {
        broken(
            subject,
            State::Error,
            &format!("skill {} could not be resolved", skill.display()),
        )
    })?;
    let resolved = fs::canonicalize(path).map_err(|_| {
        broken(
            subject,
            State::Error,
            &format!("{} could not be resolved", path.display()),
        )
    })?;
    if resolved.starts_with(root) {
        Ok(())
    } else {
        Err(broken(
            subject,
            State::Error,
            &format!("{} resolves outside its skill", path.display()),
        ))
    }
}

fn resolve(root: &Path, base: &Path, reference: &Path) -> Option<PathBuf> {
    let mut path = base.to_owned();
    for component in reference.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => path.push(part),
            Component::ParentDir if path != root => {
                path.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }
    Some(base.join(reference))
}

fn broken(subject: &str, state: State, reason: &str) -> Diagnostic {
    Diagnostic::new("skills", state, format!("broken {subject}: {reason}"))
}
