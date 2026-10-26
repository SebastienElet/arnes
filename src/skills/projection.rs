use super::references;
use crate::Roots;
use crate::diagnostic::{Diagnostic, HumanDetail, State};
use crate::files::paths::{ancestor_within, canonical_within, destination, label};
use crate::manifest::{Manifest, Scope, SkillProjection};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub fn leaf(
    roots: &Roots,
    manifest: &Manifest,
    resource: &SkillProjection<'_>,
    name: &str,
    local: bool,
) -> Vec<Diagnostic> {
    let installed = resource.destination.join(name);
    let subject = format!(
        "managed {} {} skill {name} at {}",
        resource.agent,
        resource.scope,
        label(resource.scope, &installed)
    );
    let source_root = match resource.scope {
        Scope::User => roots.deployment_repository(),
        Scope::Project => roots.repository(),
    };
    let source = source_root.join(resource.source).join(name);
    let declared = manifest
        .skill_sources(resource.source)
        .filter_map(|source| {
            source_directory(&source_root.join(source), source_root, &subject).ok()
        })
        .collect::<Vec<_>>();
    let destination = destination(roots, resource.scope, &installed);
    let human = ProjectionHuman::new(name, label(resource.scope, &installed));
    if let Err(diagnostic) = expected_link(
        roots,
        resource.scope,
        source_root,
        &source,
        &destination,
        &subject,
        &human,
    ) {
        return vec![diagnostic];
    }
    contents(&destination, &subject, &declared, name, local)
}

pub fn root(
    roots: &Roots,
    resource: &SkillProjection<'_>,
    local: bool,
) -> Result<(Vec<Diagnostic>, Vec<PathBuf>), Diagnostic> {
    let source = roots.repository().join(resource.source);
    let destination = destination(roots, resource.scope, resource.destination);
    let subject = format!(
        "managed {} {} skills projection at {}",
        resource.agent,
        resource.scope,
        label(resource.scope, resource.destination)
    );
    let human = ProjectionHuman::new(
        "managed skills projection",
        label(resource.scope, resource.destination),
    );
    expected_link(
        roots,
        resource.scope,
        roots.repository(),
        &source,
        &destination,
        &subject,
        &human,
    )?;
    let skills = super::discovery::installations(&source)
        .map_err(|reason| broken(&subject, State::Error, &reason))?;
    let diagnostics = skills
        .iter()
        .flat_map(|name| root_skill(roots, resource, &source, &destination, name, local))
        .collect();
    Ok((diagnostics, skills))
}

fn root_skill(
    roots: &Roots,
    resource: &SkillProjection<'_>,
    source: &Path,
    destination: &Path,
    name: &Path,
    local: bool,
) -> Vec<Diagnostic> {
    let source = source.join(name);
    let installed = destination.join(name);
    let subject = format!(
        "managed {} {} skill {} at {}/{}",
        resource.agent,
        resource.scope,
        name.display(),
        label(resource.scope, resource.destination),
        name.display()
    );
    let result = source_directory(&source, roots.repository(), &subject)
        .and_then(|expected| same_target(&installed, &expected, &subject));
    if let Err(diagnostic) = result {
        return vec![diagnostic];
    }
    contents(
        &installed,
        &subject,
        &[],
        &name.display().to_string(),
        local,
    )
}

fn contents(
    skill: &Path,
    subject: &str,
    declared: &[PathBuf],
    name: &str,
    local: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = references::diagnose_declared(skill, subject, declared);
    diagnostics.extend(
        super::checks::diagnose(skill, local)
            .into_iter()
            .filter(|diagnostic| diagnostic.state != State::Healthy),
    );
    if diagnostics.is_empty() {
        diagnostics.push(
            Diagnostic::new("skills", State::Healthy, format!("{subject} is current"))
                .with_human_summary(name),
        );
    }
    diagnostics
}

struct ProjectionHuman {
    summary: String,
    path: String,
}

impl ProjectionHuman {
    fn new(summary: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            path: path.into(),
        }
    }

    fn missing_destination(&self, diagnostic: Diagnostic) -> Diagnostic {
        diagnostic
            .with_human_summary(&self.summary)
            .with_human_details([
                HumanDetail::new("expected", "managed skill present"),
                HumanDetail::new("actual", "destination missing"),
                HumanDetail::new("path", &self.path),
            ])
    }
}

fn expected_link(
    roots: &Roots,
    scope: Scope,
    source_root: &Path,
    source: &Path,
    destination: &Path,
    subject: &str,
    human: &ProjectionHuman,
) -> Result<(), Diagnostic> {
    let expected = source_directory(source, source_root, subject)?;
    let boundary = match scope {
        Scope::User => roots.home(),
        Scope::Project => roots.repository(),
    };
    if !ancestor_within(destination.parent().unwrap_or(boundary), boundary) {
        return Err(broken(
            subject,
            State::Error,
            &format!(
                "destination {} escapes its scope root",
                destination.display()
            ),
        ));
    }
    let metadata = fs::symlink_metadata(destination).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            human.missing_destination(broken(
                subject,
                State::Drift,
                &format!("destination {} is missing", destination.display()),
            ))
        } else {
            broken(
                subject,
                State::Error,
                &format!("destination {} could not be read", destination.display()),
            )
        }
    })?;
    if !metadata.file_type().is_symlink() {
        return Err(broken(
            subject,
            State::Drift,
            &format!("destination {} is not a symlink", destination.display()),
        ));
    }
    same_target(destination, &expected, subject)
}

fn source_directory(source: &Path, root: &Path, subject: &str) -> Result<PathBuf, Diagnostic> {
    let metadata = fs::metadata(source).map_err(|error| {
        let reason = if error.kind() == ErrorKind::NotFound {
            "is missing"
        } else {
            "could not be read"
        };
        broken(
            subject,
            State::Error,
            &format!("source {} {reason}", source.display()),
        )
    })?;
    if !metadata.is_dir() {
        return Err(broken(
            subject,
            State::Error,
            &format!("source {} is not a directory", source.display()),
        ));
    }
    canonical_within(source, root).ok_or_else(|| {
        broken(
            subject,
            State::Error,
            &format!(
                "source {} resolves outside the repository",
                source.display()
            ),
        )
    })
}

fn same_target(path: &Path, expected: &Path, subject: &str) -> Result<(), Diagnostic> {
    match fs::canonicalize(path) {
        Ok(actual) if actual == expected => Ok(()),
        _ => Err(broken(
            subject,
            State::Drift,
            &format!(
                "destination {} has the wrong symlink target",
                path.display()
            ),
        )),
    }
}

fn broken(subject: &str, state: State, reason: &str) -> Diagnostic {
    Diagnostic::new("skills", state, format!("broken {subject}: {reason}"))
}
