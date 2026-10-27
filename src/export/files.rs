use super::ExportError;
use rustix::fs::{AtFlags, Dir, Mode, OFlags};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn validate_export_directory(output: &Path) -> Result<(), ExportError> {
    let metadata = fs::symlink_metadata(output).map_err(|error| {
        ExportError::new(format!(
            "export directory does not exist or is unreadable: {error}"
        ))
    })?;
    if !metadata.file_type().is_dir() {
        return Err(ExportError::new("export path is not an owned directory"));
    }
    let parent = output
        .parent()
        .ok_or_else(|| ExportError::new("export directory has no parent"))?;
    let expected = fs::canonicalize(parent)
        .map_err(|error| ExportError::new(format!("export parent is unreadable: {error}")))?
        .join(
            output
                .file_name()
                .ok_or_else(|| ExportError::new("export directory has no name"))?,
        );
    let resolved = fs::canonicalize(output)
        .map_err(|error| ExportError::new(format!("export directory is unreadable: {error}")))?;
    if resolved != expected {
        return Err(ExportError::new("export directory escapes its parent"));
    }
    Ok(())
}

pub(super) fn check_snapshot(
    output: &Path,
    expected: &BTreeMap<String, String>,
) -> Result<(), ExportError> {
    let entries = fs::read_dir(output).map_err(|error| {
        ExportError::new(format!(
            "export directory does not exist or is unreadable: {error}"
        ))
    })?;
    let mut actual_names = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            ExportError::new(format!("export directory could not be inspected: {error}"))
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ExportError::new("export directory contains a non-UTF-8 artifact name"))?;
        if !entry
            .file_type()
            .map_err(|error| ExportError::new(format!("{name} could not be inspected: {error}")))?
            .is_file()
        {
            return Err(ExportError::new(format!(
                "unexpected non-file export artifact: {name}"
            )));
        }
        actual_names.insert(name);
    }
    let expected_names = expected.keys().cloned().collect::<BTreeSet<_>>();
    if let Some(name) = actual_names.difference(&expected_names).next() {
        return Err(ExportError::new(format!(
            "unexpected export artifact: {name}"
        )));
    }
    if let Some(name) = expected_names.difference(&actual_names).next() {
        return Err(ExportError::new(format!(
            "export artifact {name} is missing"
        )));
    }
    for (name, expected_contents) in expected {
        let actual = fs::read(output.join(name)).map_err(|error| {
            ExportError::new(format!("export artifact {name} could not be read: {error}"))
        })?;
        if actual != expected_contents.as_bytes() {
            return Err(ExportError::new(format!("export artifact {name} is stale")));
        }
    }
    Ok(())
}

pub(super) fn publish_snapshot(
    output: &Path,
    snapshot: &BTreeMap<String, String>,
) -> Result<(), ExportError> {
    let directory = open_export_directory(output)?;
    let previous = artifact_names(&directory)?;
    for (name, contents) in snapshot
        .iter()
        .filter(|(name, _)| name.as_str() != "00-MANIFEST.md")
    {
        replace_artifact(&directory, name, contents)?;
    }
    for name in previous {
        if !snapshot
            .keys()
            .any(|expected| expected.as_bytes() == name.as_bytes())
        {
            rustix::fs::unlinkat(&directory, name.as_c_str(), AtFlags::empty())
                .map_err(export_io_error)?;
        }
    }
    if let Some(contents) = snapshot.get("00-MANIFEST.md") {
        replace_artifact(&directory, "00-MANIFEST.md", contents)?;
    }
    Ok(())
}

fn open_export_directory(output: &Path) -> Result<File, ExportError> {
    let parent = output
        .parent()
        .ok_or_else(|| ExportError::new("export directory has no parent"))?;
    let name = output
        .file_name()
        .ok_or_else(|| ExportError::new("export directory has no name"))?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    let parent = rustix::fs::open(parent, flags, Mode::empty()).map_err(export_io_error)?;
    match rustix::fs::mkdirat(&parent, name, Mode::from_raw_mode(0o755)) {
        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
        Err(error) => return Err(export_io_error(error)),
    }
    rustix::fs::openat(&parent, name, flags, Mode::empty())
        .map(File::from)
        .map_err(|error| {
            ExportError::new(format!("export path is not an owned directory: {error}"))
        })
}

fn artifact_names(directory: &File) -> Result<Vec<CString>, ExportError> {
    let entries = Dir::read_from(directory).map_err(export_io_error)?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(export_io_error)?;
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let metadata = rustix::fs::statat(directory, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(export_io_error)?;
        if rustix::fs::FileType::from_raw_mode(metadata.st_mode) == rustix::fs::FileType::Directory
        {
            return Err(ExportError::new("unexpected directory in export output"));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

fn replace_artifact(directory: &File, name: &str, contents: &str) -> Result<(), ExportError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ExportError::new("system clock is before the Unix epoch"))?
        .as_nanos();
    let temporary = format!(".arnes-export-{}-{nonce}", std::process::id());
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    let mut file = File::from(
        rustix::fs::openat(
            directory,
            temporary.as_str(),
            flags,
            Mode::from_raw_mode(0o600),
        )
        .map_err(export_io_error)?,
    );
    let result = file.write_all(contents.as_bytes()).and_then(|()| {
        rustix::fs::renameat(directory, temporary.as_str(), directory, name)
            .map_err(io::Error::from)
    });
    if result.is_err() {
        rustix::fs::unlinkat(directory, temporary.as_str(), AtFlags::empty())
            .map_err(export_io_error)?;
    }
    result.map_err(|error| {
        ExportError::new(format!(
            "export artifact {name} could not be written: {error}"
        ))
    })
}

fn export_io_error(error: rustix::io::Errno) -> ExportError {
    ExportError::new(format!("export output could not be updated: {error}"))
}
