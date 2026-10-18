use super::{MANIFEST_FILE, Manifest, ManifestData, ManifestError, validation};
use serde_yaml_ng::Value;
use std::fs;
use std::path::Path;

/// # Errors
/// Returns an error when the home manifest cannot be read or fails manifest validation.
pub fn load(home: &Path) -> Result<Manifest, ManifestError> {
    let path = path(home)?;
    let manifest = fs::read_to_string(path).map_err(|error| {
        let reason = if error.kind() == std::io::ErrorKind::NotFound {
            format!("{MANIFEST_FILE} was not found")
        } else {
            format!("could not read {MANIFEST_FILE}")
        };
        ManifestError::new("manifest", reason)
    })?;
    parse(&manifest)
}

/// # Errors
/// Rejects an explicit manifest path that is not absolute.
pub fn path(home: &Path) -> Result<std::path::PathBuf, ManifestError> {
    match std::env::var_os("ARNES_MANIFEST") {
        None => Ok(home.join(MANIFEST_FILE)),
        Some(value) => {
            let path = std::path::PathBuf::from(value);
            if !path.is_absolute() {
                return Err(ManifestError::new(
                    "manifest",
                    "ARNES_MANIFEST must be an absolute file path",
                ));
            }
            Ok(path)
        }
    }
}

/// # Errors
/// Rejects malformed YAML, unsupported versions, secret fields, or invalid resource and installation declarations.
pub fn parse(input: &str) -> Result<Manifest, ManifestError> {
    let value: Value = serde_yaml_ng::from_str(input)
        .map_err(|error| ManifestError::new("manifest", error.to_string()))?;
    validation::validate_value(&value)?;
    if let Some(field) = validation::secret_field(&value, "") {
        return Err(ManifestError::new(field, "secret values are not allowed"));
    }
    let deserializer = serde_yaml_ng::Deserializer::from_str(input);
    let data: ManifestData = serde_path_to_error::deserialize(deserializer).map_err(|error| {
        let field = error.path().to_string();
        ManifestError::new(
            if field.is_empty() || field == "." {
                "manifest"
            } else {
                &field
            },
            error.into_inner().to_string(),
        )
    })?;
    let manifest = Manifest(data);
    validation::validate(&manifest)?;
    Ok(manifest)
}
