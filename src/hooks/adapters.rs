use crate::manifest::Agent;

const CODEX: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "SessionEnd",
];
const CLAUDE: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolUse",
    "PostToolUseFailure",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "StopFailure",
    "PreCompact",
    "PostCompact",
    "SessionEnd",
];
const CURSOR: &[&str] = &[
    "sessionStart",
    "beforeSubmitPrompt",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "subagentStart",
    "subagentStop",
    "afterAgentResponse",
    "stop",
    "preCompact",
    "postCompact",
    "sessionEnd",
];

pub struct Policy {
    pub directory: &'static str,
    pub filename: &'static str,
    pub events: &'static [&'static str],
    pub nested: bool,
    pub memory_event: Option<&'static str>,
    pub format_edited_file_matcher: Option<&'static str>,
}

pub const fn policy(agent: Agent) -> Policy {
    match agent {
        Agent::Codex => Policy {
            directory: ".codex",
            filename: "hooks.json",
            events: CODEX,
            nested: true,
            memory_event: Some("UserPromptSubmit"),
            format_edited_file_matcher: Some("apply_patch"),
        },
        Agent::Claude => Policy {
            directory: ".claude",
            filename: "settings.json",
            events: CLAUDE,
            nested: true,
            memory_event: Some("UserPromptSubmit"),
            format_edited_file_matcher: Some("Edit|Write|MultiEdit"),
        },
        Agent::Cursor => Policy {
            directory: ".cursor",
            filename: "hooks.json",
            events: CURSOR,
            nested: false,
            memory_event: None,
            format_edited_file_matcher: None,
        },
    }
}
