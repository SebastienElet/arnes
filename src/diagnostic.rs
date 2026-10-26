use serde::Serialize;
use std::fmt::{self, Display};

mod human;

pub use human::{ColorMode, HumanContext, HumanOptions};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Healthy,
    Warning,
    Drift,
    Unsupported,
    Error,
}

impl Display for State {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Healthy => "healthy",
            Self::Warning => "warning",
            Self::Drift => "drift",
            Self::Unsupported => "unsupported",
            Self::Error => "error",
        })
    }
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub resource: String,
    pub state: State,
    pub message: String,
    #[serde(skip)]
    human: Option<Box<HumanDiagnostic>>,
    #[serde(skip)]
    section: Option<Box<HumanSection>>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    finding: Option<Box<Finding>>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct Finding {
    file: std::path::PathBuf,
    line: usize,
    rule: &'static str,
    measurement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tokenizer: Option<&'static str>,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct HumanDiagnostic {
    group: String,
    summary: String,
    details: Vec<HumanDetail>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanDetail {
    label: String,
    value: String,
}

impl HumanDetail {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanSection {
    key: String,
    label: String,
}

impl HumanSection {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }

    pub(super) fn key(&self) -> &str {
        &self.key
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }
}

impl HumanDiagnostic {
    pub(super) fn group(&self) -> &str {
        &self.group
    }

    pub(super) fn summary(&self) -> &str {
        &self.summary
    }

    pub(super) fn details(&self) -> &[HumanDetail] {
        &self.details
    }
}

impl Diagnostic {
    pub fn new(resource: impl Into<String>, state: State, message: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            state,
            message: message.into(),
            human: None,
            section: None,
            finding: None,
        }
    }

    #[must_use]
    pub(crate) fn with_finding(
        mut self,
        file: &std::path::Path,
        line: usize,
        rule: &'static str,
        measurement: impl Into<String>,
        tokenizer: Option<&'static str>,
    ) -> Self {
        let measurement = measurement.into();
        self = self.with_human_summary(format!(
            "{}:{line} {rule} {measurement}{}",
            file.display(),
            tokenizer.map_or_else(String::new, |name| format!(" {name}"))
        ));
        self.finding = Some(Box::new(Finding {
            file: file.to_path_buf(),
            line,
            rule,
            measurement,
            tokenizer,
        }));
        self
    }

    #[must_use]
    pub fn with_human(mut self, group: impl Into<String>, summary: impl Into<String>) -> Self {
        self.human = Some(Box::new(HumanDiagnostic {
            group: group.into(),
            summary: summary.into(),
            details: Vec::new(),
        }));
        self
    }

    #[must_use]
    pub fn with_human_summary(mut self, summary: impl Into<String>) -> Self {
        let summary = summary.into();
        match &mut self.human {
            Some(human) => human.summary = summary,
            None => {
                self.human = Some(Box::new(HumanDiagnostic {
                    group: String::new(),
                    summary,
                    details: Vec::new(),
                }));
            }
        }
        self
    }

    #[must_use]
    pub fn with_human_details(mut self, details: impl IntoIterator<Item = HumanDetail>) -> Self {
        let human = self.human.get_or_insert_with(|| {
            Box::new(HumanDiagnostic {
                group: String::new(),
                summary: self.message.clone(),
                details: Vec::new(),
            })
        });
        human.details = details.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_human_section(mut self, section: HumanSection) -> Self {
        self.section = Some(Box::new(section));
        self
    }

    pub(super) fn human(&self) -> Option<&HumanDiagnostic> {
        self.human.as_deref()
    }

    pub(super) fn section(&self) -> Option<&HumanSection> {
        self.section.as_deref()
    }
}

impl Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {}: {}",
            self.state,
            human_field(&self.resource),
            human_field(&self.message)
        )
    }
}

pub(super) fn human_field(value: &str) -> String {
    value.replace('\r', "\\r").replace('\n', "\\n")
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct Report {
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    #[must_use]
    pub const fn new(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    #[must_use]
    pub fn exit_code(&self) -> u8 {
        self.diagnostics
            .iter()
            .map(|diagnostic| match diagnostic.state {
                State::Error => 2,
                State::Drift => 1,
                State::Healthy | State::Unsupported | State::Warning => 0,
            })
            .max()
            .unwrap_or(0)
    }

    #[must_use]
    pub fn human(&self, context: &HumanContext, options: HumanOptions) -> String {
        human::render(&self.diagnostics, context, options)
    }

    /// # Errors
    /// Returns a serialization error if the diagnostic report cannot be encoded as JSON.
    pub fn json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.diagnostics)
    }
}
