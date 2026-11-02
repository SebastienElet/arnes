use super::manifest;
use super::model::{Exposure, Plugin, Topology, plugin_diagnostics};
use crate::Roots;
use crate::diagnostic::{Diagnostic, State};
use crate::files::paths::{ancestor_within, canonical_within};
use crate::manifest::{Agent, Manifest, Scope};
use std::fs;
use std::fs::DirEntry;
use std::io::ErrorKind;
use std::path::Path;

pub(super) fn diagnose(roots: &Roots, policy: &Manifest, scope: Scope) -> Vec<Diagnostic> {
    if scope == Scope::Project {
        return Vec::new();
    }
    let root = roots.home().join(".cursor/plugins/local");
    match local_plugins(&root, roots, policy) {
        Ok(plugins) => plugin_diagnostics(policy, Agent::Cursor, scope, plugins),
        Err(detail) => vec![Diagnostic::new(
            "skills",
            State::Error,
            format!(
                "external cursor user local plugin root origin=plugin ownership=external exposure=unknown topology=unreadable policy=unknown activation=unknown path={} detail={detail}",
                root.display(),
            ),
        )],
    }
}

fn local_plugins(
    root: &Path,
    roots: &Roots,
    policy: &Manifest,
) -> Result<Vec<Plugin>, &'static str> {
    let home = roots.home();
    match fs::symlink_metadata(root) {
        Ok(_) if canonical_within(root, home).is_none() => {
            return Err("local plugin root resolves outside HOME");
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return if ancestor_within(root, home) {
                Ok(Vec::new())
            } else {
                Err("local plugin root has an ancestor outside HOME")
            };
        }
        Err(_) => return Err("local plugin root metadata could not be read"),
    }
    let entries = fs::read_dir(root).map_err(|_| "local plugin root could not be read")?;
    let mut plugins = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "local plugin root entry could not be read")?
        .into_iter()
        .filter_map(|entry| inspect_plugin(root, &entry, roots, policy))
        .collect::<Vec<_>>();
    plugins.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(plugins)
}

fn inspect_plugin(
    root: &Path,
    entry: &DirEntry,
    roots: &Roots,
    policy: &Manifest,
) -> Option<Plugin> {
    match entry.file_type() {
        Ok(kind) if !kind.is_dir() && !kind.is_symlink() => return None,
        Err(_) => {
            return Some(Plugin {
                id: entry.file_name().to_string_lossy().into_owned(),
                artifact: None,
                version: None,
                path: Some(entry.path()),
                exposure: Exposure::Unknown,
                topology: Topology::Unreadable,
                detail: Some("local plugin metadata could not be read".into()),
                skills: Vec::new(),
            });
        }
        Ok(_) => {}
    }
    let path = entry.path();
    if fs::metadata(&path).is_err() || canonical_within(&path, root).is_none() {
        return Some(Plugin {
            id: entry.file_name().to_string_lossy().into_owned(),
            artifact: None,
            version: None,
            path: Some(path),
            exposure: Exposure::Enabled,
            topology: Topology::Broken,
            detail: Some("local plugin link is dangling or escapes its root".into()),
            skills: Vec::new(),
        });
    }
    let candidates = ["plugin.json", ".cursor-plugin/plugin.json"];
    let projections = candidates
        .iter()
        .filter_map(|candidate| {
            let destination = path.join(candidate);
            let relative = destination.strip_prefix(roots.home()).ok()?;
            let source = policy.configuration_source(Agent::Cursor, Scope::User, relative)?;
            let canonical_source = canonical_within(
                &roots.deployment_repository().join(source),
                roots.deployment_repository(),
            )?;
            Some(manifest::ManifestProjection {
                destination,
                source: canonical_source,
                relative_source: source.to_owned(),
            })
        })
        .collect::<Vec<_>>();
    let inspected = manifest::inspect_projected(
        &path,
        &candidates,
        &projections,
        roots.deployment_repository(),
    );
    let id = inspected
        .name
        .clone()
        .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
    let missing_name = inspected.name.is_none() && inspected.topology == Topology::Healthy;
    let detail = if missing_name && inspected.detail.is_none() {
        Some("plugin manifest name is missing".to_owned())
    } else {
        inspected.detail
    };
    Some(Plugin {
        id,
        artifact: None,
        version: inspected.version,
        path: Some(path),
        exposure: Exposure::Enabled,
        topology: if missing_name {
            Topology::Broken
        } else {
            inspected.topology
        },
        detail,
        skills: inspected.skills,
    })
}
