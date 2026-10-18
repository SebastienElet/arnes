use std::env;
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq)]
pub struct Roots {
    repository: PathBuf,
    deployment_repository: PathBuf,
    home: PathBuf,
}

impl Roots {
    pub fn new(repository: impl Into<PathBuf>, home: impl Into<PathBuf>) -> Self {
        let repository = repository.into();
        Self {
            deployment_repository: repository.clone(),
            repository,
            home: home.into(),
        }
    }

    /// # Errors
    /// Rejects missing, empty, or relative HOME values and returns current-directory or deployment-root resolution errors.
    pub fn from_environment() -> Result<Self, RootsError> {
        let repository = match env::var_os("ARNES_REPOSITORY_ROOT") {
            Some(value) => explicit_directory(value, "ARNES_REPOSITORY_ROOT")?,
            None => env::current_dir()
                .map_err(|_| RootsError::new("repository: current directory is unavailable"))?,
        };
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| RootsError::new("HOME: environment variable is required"))?;
        if home.as_os_str().is_empty() {
            return Err(RootsError::new(
                "HOME: environment variable cannot be empty",
            ));
        }
        if !home.is_absolute() {
            return Err(RootsError::new(
                "HOME: environment variable must be an absolute path",
            ));
        }
        let deployment_repository = match env::var_os("ARNES_SOURCE_ROOT") {
            Some(value) => explicit_directory(value, "ARNES_SOURCE_ROOT")?,
            None => repository.clone(),
        };
        Ok(Self {
            repository,
            deployment_repository,
            home,
        })
    }

    #[must_use]
    pub fn repository(&self) -> &Path {
        &self.repository
    }

    #[must_use]
    pub fn deployment_repository(&self) -> &Path {
        &self.deployment_repository
    }

    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct RootsError(String);

impl RootsError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for RootsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RootsError {}

fn explicit_directory(value: std::ffi::OsString, name: &str) -> Result<PathBuf, RootsError> {
    let path = PathBuf::from(value);
    if !path.is_absolute() || !path.is_dir() {
        return Err(RootsError::new(format!(
            "{name} must be an existing absolute directory"
        )));
    }
    path.canonicalize()
        .map_err(|error| RootsError::new(error.to_string()))
}
