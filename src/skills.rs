use crate::Roots;
use crate::diagnostic::{Diagnostic, HumanSection, State};
use crate::manifest::{Agent, Manifest, Scope, SkillLayout, SkillProjection};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

mod checks;
pub(crate) mod discovery;
mod external;
mod projection;
pub(crate) mod references;

#[must_use]
pub fn diagnose(
    roots: &Roots,
    manifest: &Manifest,
    agent: Option<Agent>,
    scope: Option<Scope>,
) -> Vec<Diagnostic> {
    diagnose_with_local(roots, manifest, agent, scope, false)
}

#[must_use]
pub fn diagnose_local(
    roots: &Roots,
    manifest: &Manifest,
    agent: Option<Agent>,
    scope: Option<Scope>,
) -> Vec<Diagnostic> {
    diagnose_with_local(roots, manifest, agent, scope, true)
}

#[must_use]
pub fn diagnose_path(directory: &Path, local: bool) -> Vec<Diagnostic> {
    let names = match discovery::installations(directory) {
        Ok(names) => names,
        Err(reason) => {
            return vec![checks::finding(
                directory,
                1,
                "discovery",
                reason,
                State::Error,
            )];
        }
    };
    names
        .into_iter()
        .flat_map(|name| {
            let skill = directory.join(name);
            let file = skill.join("SKILL.md");
            match std::fs::symlink_metadata(&file) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
                Err(_) => {
                    return vec![checks::finding(
                        &file,
                        1,
                        "discovery",
                        "SKILL.md could not be inspected",
                        State::Error,
                    )];
                }
            }
            let subject = format!("skill {}", skill.display());
            let mut diagnostics = references::diagnose_declared(&skill, &subject, &[]);
            diagnostics.extend(checks::diagnose(&skill, local));
            diagnostics.extend(checks::dated_files(&skill, &subject));
            diagnostics
        })
        .collect()
}

fn diagnose_with_local(
    roots: &Roots,
    manifest: &Manifest,
    agent: Option<Agent>,
    scope: Option<Scope>,
    local: bool,
) -> Vec<Diagnostic> {
    let undeclared_agents = external::outside_manifest(roots, manifest, agent, scope);
    let combinations = manifest
        .combinations()
        .filter(|(candidate, _)| agent.is_none_or(|agent| agent == *candidate))
        .filter(|(_, candidate)| scope.is_none_or(|scope| scope == *candidate))
        .collect::<Vec<_>>();
    if combinations.is_empty() {
        let diagnostic = unsupported(agent, scope);
        return std::iter::once(match (agent, scope) {
            (Some(agent), Some(scope)) => diagnostic.with_human_section(section(agent, scope)),
            _ => diagnostic,
        })
        .chain(undeclared_agents)
        .collect();
    }
    combinations
        .into_iter()
        .flat_map(|(agent, scope)| {
            diagnose_one(roots, manifest, agent, scope, local)
                .into_iter()
                .map(move |diagnostic| diagnostic.with_human_section(section(agent, scope)))
        })
        .chain(undeclared_agents)
        .collect()
}

fn section(agent: Agent, scope: Scope) -> HumanSection {
    HumanSection::new(format!("{agent}:{scope}"), agent.to_string().to_uppercase())
}

fn diagnose_one(
    roots: &Roots,
    manifest: &Manifest,
    agent: Agent,
    scope: Scope,
    local: bool,
) -> Vec<Diagnostic> {
    let resources = manifest
        .skill_projections()
        .filter(|resource| resource.agent == agent && resource.scope == scope)
        .collect::<Vec<_>>();
    if resources.is_empty() {
        return std::iter::once(unsupported(Some(agent), Some(scope)))
            .chain(external::diagnose(roots, manifest, agent, scope))
            .collect();
    }
    let (supported, unsupported_resources): (Vec<_>, Vec<_>) =
        resources.into_iter().partition(declaration_supported);
    let mut diagnostics = unsupported_resources
        .into_iter()
        .map(|resource| unsupported_resource(&resource))
        .collect::<Vec<_>>();
    for resource in supported {
        match resource.layout {
            SkillLayout::Leaves => {
                diagnostics.extend(diagnose_leaves(roots, manifest, &resource, local));
            }
            SkillLayout::Root => {
                diagnostics.extend(diagnose_root(roots, manifest, &resource, local));
            }
        }
    }
    diagnostics.extend(external::diagnose(roots, manifest, agent, scope));
    diagnostics
}

fn diagnose_leaves(
    roots: &Roots,
    manifest: &Manifest,
    resource: &SkillProjection<'_>,
    local: bool,
) -> Vec<Diagnostic> {
    let skills = manifest
        .installed_skills(resource.agent, resource.scope)
        .collect::<Vec<_>>();
    let declared = skills
        .iter()
        .map(|skill| resource.destination.join(*skill))
        .collect::<HashSet<PathBuf>>();
    let mut diagnostics = skills
        .iter()
        .flat_map(|skill| projection::leaf(roots, manifest, resource, skill, local))
        .collect::<Vec<_>>();
    diagnostics.extend(discovery::unmanaged(
        roots,
        resource.agent,
        resource.scope,
        resource.destination,
        &declared,
        manifest,
    ));
    diagnostics
}

fn diagnose_root(
    roots: &Roots,
    manifest: &Manifest,
    resource: &SkillProjection<'_>,
    local: bool,
) -> Vec<Diagnostic> {
    let (diagnostics, skills) = match projection::root(roots, resource, local) {
        Ok(result) => result,
        Err(diagnostic) => return vec![diagnostic],
    };
    let declared = skills
        .iter()
        .map(|skill| resource.destination.join(skill))
        .collect::<HashSet<PathBuf>>();
    let mut diagnostics = diagnostics;
    diagnostics.extend(discovery::unmanaged(
        roots,
        resource.agent,
        resource.scope,
        resource.destination,
        &declared,
        manifest,
    ));
    diagnostics
}

fn declaration_supported(resource: &SkillProjection<'_>) -> bool {
    matches!(
        (resource.scope, resource.layout, resource.source),
        (Scope::User, SkillLayout::Leaves, source) if source == Path::new("harness/skills")
    ) || matches!(
        (resource.scope, resource.layout, resource.source),
        (Scope::Project, SkillLayout::Root, source) if source == Path::new(".agents/skills")
    )
}

fn unsupported(agent: Option<Agent>, scope: Option<Scope>) -> Diagnostic {
    let subject = match (agent, scope) {
        (Some(agent), Some(scope)) => format!("{agent} {scope} skill projection"),
        (Some(agent), None) => format!("{agent} skill projections"),
        (None, Some(scope)) => format!("{scope} skill scope"),
        (None, None) => "skill projections".to_owned(),
    };
    Diagnostic::new(
        "skills",
        State::Unsupported,
        format!("{subject} is not declared or supported"),
    )
}

fn unsupported_resource(resource: &SkillProjection<'_>) -> Diagnostic {
    Diagnostic::new(
        "skills",
        State::Unsupported,
        format!(
            "{} {} skill projection {} from {} to {} is unsupported",
            resource.agent,
            resource.scope,
            resource.id,
            resource.source.display(),
            resource.destination.display()
        ),
    )
}
