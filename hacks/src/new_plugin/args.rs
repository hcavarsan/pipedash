use std::path::PathBuf;

use clap::{
    Parser,
    ValueEnum,
};
use serde::Serialize;

#[derive(Parser, Clone, Default)]
#[command(
    name = "new_plugin",
    about = "Scaffold a Pipedash provider plugin crate"
)]
pub(crate) struct Args {
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long, value_enum)]
    pub archetype: Option<Archetype>,
    #[arg(long, value_enum)]
    pub id_shape: Option<IdShape>,
    #[arg(long, value_enum, value_delimiter = ',')]
    pub capabilities: Option<Vec<Capability>>,
    #[arg(long, conflicts_with = "capabilities")]
    pub read_only: bool,
    #[arg(long, value_delimiter = ',')]
    pub aliases: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    pub token_prefixes: Option<Vec<String>>,
    #[arg(long)]
    pub token_label: Option<String>,
    #[arg(long)]
    pub token_help: Option<String>,
    #[arg(long)]
    pub ping_path: Option<String>,
    #[arg(long)]
    pub icon: Option<PathBuf>,
    #[arg(long)]
    pub no_input: bool,
    #[arg(long, short)]
    pub yes: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(ValueEnum, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Archetype {
    Rest,
    Kubernetes,
}

#[derive(ValueEnum, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IdShape {
    Numeric,
    Path,
    Namespaced,
}

#[derive(ValueEnum, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Capability {
    Trigger,
    Cancel,
    Logs,
}
