use arnes::diagnostic::ColorMode;
use arnes::manifest::{Agent, Scope};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    version,
    about = "Verify agent harness configuration without modifying it"
)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Export sources only into .harness-export; never install configuration")]
    Export {
        #[arg(long)]
        sources: Option<std::path::PathBuf>,
        #[arg(long)]
        check: bool,
    },
    #[command(
        about = "Report configuration drift without writing; installation belongs to your dotfiles manager"
    )]
    Doctor(DoctorArgs),
}

#[derive(clap::Args)]
pub struct DoctorArgs {
    #[arg(value_enum)]
    pub resource: Option<Resource>,
    #[arg(long, value_enum)]
    pub agent: Option<Agent>,
    #[arg(long, value_enum)]
    pub scope: Option<Scope>,
    #[arg(long, value_enum, default_value_t)]
    pub format: Format,
    #[arg(long, value_enum, default_value_t)]
    pub color: Color,
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(long, conflicts_with_all = ["agent", "scope"])]
    pub path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub local: bool,
}

#[derive(Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum Resource {
    Manifest,
    Config,
    Instructions,
    Skills,
    Prompts,
    Commands,
    Rules,
    Hooks,
    Mcp,
    Statusline,
}

#[derive(Clone, Copy, Default, Eq, PartialEq, ValueEnum)]
pub enum Format {
    #[default]
    Human,
    Json,
}

#[derive(Clone, Copy, Default, Eq, PartialEq, ValueEnum)]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

impl From<Color> for ColorMode {
    fn from(color: Color) -> Self {
        match color {
            Color::Auto => Self::Auto,
            Color::Always => Self::Always,
            Color::Never => Self::Never,
        }
    }
}

/// # Errors
/// Returns an error when verbose or forced-color output is combined with JSON.
pub fn validate_render_options(
    format: Format,
    verbose: bool,
    color: Color,
) -> Result<(), &'static str> {
    if verbose && format == Format::Json {
        return Err("--verbose cannot be used with --format json");
    }
    if color == Color::Always && format == Format::Json {
        return Err("--color always cannot be used with --format json");
    }
    Ok(())
}
