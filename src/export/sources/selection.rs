use super::{Category, ExportError, decode_git_paths, reject_sensitive_path};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

pub(super) fn read(
    repository: &Path,
    path: &Path,
) -> Result<BTreeMap<String, Category>, ExportError> {
    let bytes =
        fs::read(path).map_err(|error| ExportError::new(format!("export selection: {error}")))?;
    let selected: BTreeMap<String, Category> = serde_json::from_slice(&bytes)
        .map_err(|error| ExportError::new(format!("export selection: {error}")))?;
    if selected.is_empty() {
        return Err(ExportError::new("empty export selection"));
    }
    for path in selected.keys() {
        decode_git_paths(format!("{path}\0").into_bytes())?;
        if Path::new(path)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || Path::new(path).starts_with(".harness-export")
        {
            return Err(ExportError::new(
                "export source must be a relative input path",
            ));
        }
        reject_sensitive_path(path)?;
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(["check-ignore", "--quiet", "--", path])
            .output()
            .map_err(|error| ExportError::new(error.to_string()))?;
        match output.status.code() {
            Some(1) => (),
            Some(0) => return Err(ExportError::new(format!("refusing ignored source {path}"))),
            _ => return Err(ExportError::new("could not inspect source ignore status")),
        }
    }
    Ok(selected)
}
