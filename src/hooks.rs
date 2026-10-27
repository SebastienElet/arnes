use crate::manifest::Agent;
use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

mod adapters;
mod inspect;
mod json_value;
mod validate;

const OUTPUT_DISCIPLINE: MatchedHandler = MatchedHandler {
    event: "SessionStart",
    matcher: "startup|resume|clear|compact",
    timeout_seconds: 30,
};
const SEMCTX_NUDGE: MatchedHandler = MatchedHandler {
    event: "SessionStart",
    matcher: "startup|resume|clear|compact",
    timeout_seconds: 5,
};
const FORMAT_EDITED_FILE_EVENT: &str = "PostToolUse";
const FORMAT_EDITED_FILE_TIMEOUT_SECONDS: u64 = 30;
pub use inspect::diagnose;

pub struct MatchedHandler {
    pub event: &'static str,
    pub matcher: &'static str,
    pub timeout_seconds: u64,
}

fn semctx_nudge_path(repository: &Path) -> PathBuf {
    repository.join("tooling/semctx-nudge")
}

fn semctx_nudge_host(agent: Agent) -> Result<&'static str, HooksError> {
    match agent {
        Agent::Claude => Ok("claude"),
        Agent::Codex => Ok("codex"),
        Agent::Cursor => Err(HooksError::new(
            "Cursor does not support the semctx-nudge hook",
        )),
    }
}

fn semctx_nudge_command(path: &Path, agent: Agent) -> Result<String, HooksError> {
    Ok(format!(
        "{} --host {}",
        quoted_command(path)?,
        semctx_nudge_host(agent)?
    ))
}

fn unquoted_semctx_nudge_command(path: &Path, agent: Agent) -> Result<String, HooksError> {
    Ok(format!(
        "{} --host {}",
        path_string(path)?,
        semctx_nudge_host(agent)?
    ))
}

fn format_edited_file_path(repository: &Path) -> PathBuf {
    repository.join("tooling/format-edited-file")
}

fn format_edited_file_handler(agent: Agent) -> Result<MatchedHandler, HooksError> {
    let matcher = adapters::policy(agent)
        .format_edited_file_matcher
        .ok_or_else(|| HooksError::new("Cursor does not support the format-edited-file hook"))?;
    Ok(MatchedHandler {
        event: FORMAT_EDITED_FILE_EVENT,
        matcher,
        timeout_seconds: FORMAT_EDITED_FILE_TIMEOUT_SECONDS,
    })
}

fn quoted_command(command: &Path) -> Result<String, HooksError> {
    let command = path_string(command)?;
    Ok(format!("'{}'", command.replace('\'', "'\\''")))
}

fn measurement_path(home: &Path) -> PathBuf {
    home.join(".local/bin/agent-measure")
}

fn handoff_path(home: &Path) -> PathBuf {
    home.join(".local/bin/agent-handoff")
}

fn memory_path(home: &Path) -> PathBuf {
    home.join(".local/bin/agent-memory")
}

fn handoff_aliases(
    command: &Path,
    repository: &Path,
    agent: Agent,
) -> Result<Vec<String>, HooksError> {
    let mut aliases = vec![path_string(command)?];
    aliases
        .extend((agent == Agent::Claude).then_some("$HOME/.claude/hooks/agent_handoff".to_owned()));
    aliases.extend([
        path_string(&repository.join("tooling/agent-handoff"))?,
        path_string(&repository.join("scripts/agent_handoff"))?,
    ]);
    let metadata = match fs::symlink_metadata(command) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(aliases),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_symlink() {
        return Ok(aliases);
    }
    let target = fs::read_link(command)?;
    let target = if target.is_absolute() {
        target
    } else {
        command
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .join(target)
    };
    aliases.push(path_string(&target)?);
    aliases.dedup();
    Ok(aliases)
}

const fn measurement_agent(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude-code",
        Agent::Codex => "codex",
        Agent::Cursor => "cursor",
    }
}

fn measurement_command(command: &Path, agent: Agent) -> Result<String, HooksError> {
    let command = path_string(command)?;
    let quoted = format!("'{}'", command.replace('\'', "'\\''"));
    Ok(format!(
        "{quoted} hook --agent {}",
        measurement_agent(agent)
    ))
}

fn output_discipline_command(command: &Path) -> Result<String, HooksError> {
    let command = path_string(command)?;
    let quoted = format!("'{}'", command.replace('\'', "'\\''"));
    Ok(quoted)
}

fn memory_command(command: &Path, agent: Agent) -> Result<Option<String>, HooksError> {
    let agent = match agent {
        Agent::Codex => "codex",
        Agent::Claude => "claude",
        Agent::Cursor => return Ok(None),
    };
    let command = path_string(command)?;
    let quoted = format!("'{}'", command.replace('\'', "'\\''"));
    Ok(Some(format!("{quoted} hook --agent {agent}")))
}

fn path_string(path: &Path) -> Result<String, HooksError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| HooksError::new("hook command path must be valid UTF-8"))
}

#[derive(Debug)]
pub struct HooksError(String);

impl HooksError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for HooksError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for HooksError {}

impl From<std::io::Error> for HooksError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for HooksError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::handoff_aliases;
    use std::path::Path;

    #[test]
    fn handoff_aliases_include_repository_runtimes() -> Result<(), Box<dyn std::error::Error>> {
        let aliases = handoff_aliases(
            Path::new("/tmp/home/.local/bin/agent-handoff"),
            Path::new("/tmp/repository"),
            crate::manifest::Agent::Claude,
        )?;

        assert!(aliases.contains(&"/tmp/repository/tooling/agent-handoff".to_owned()));
        assert!(aliases.contains(&"/tmp/repository/scripts/agent_handoff".to_owned()));
        Ok(())
    }
}
