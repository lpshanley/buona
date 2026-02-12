mod config;
mod styles;
mod workspace;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "buona", version, about = "The Good CLI — making life easier when managing complex workspace and build tasks", arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// View or set up the global configuration
    #[command(arg_required_else_help = true)]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Manage workspaces
    #[command(alias = "ws", arg_required_else_help = true)]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Display the current configuration
    Show {
        /// Print config as JSON
        #[arg(long)]
        json: bool,
    },

    /// Launch interactive setup wizard
    Setup,
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    /// List all workspaces in the configured directory
    List,

    /// Create a new workspace
    Create {
        /// Path for the new workspace (relative to the configured workspace directory, or absolute)
        path: String,

        /// Optional display name for the workspace (defaults to the directory name)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Remove a workspace
    Remove {
        /// Name or directory of the workspace to remove
        workspace: String,

        /// Skip the confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { command } => match command {
            ConfigCommands::Show { json } => {
                let cfg = config::load_config()?;
                if json {
                    config::print_json(&cfg)?;
                } else {
                    config::print_pretty(&cfg)?;
                }
            }
            ConfigCommands::Setup => {
                config::run_setup()?;
            }
        },
        Commands::Workspace { command } => match command {
            WorkspaceCommands::List => {
                workspace::list()?;
            }
            WorkspaceCommands::Create { path, name } => {
                workspace::create(&path, name.as_deref())?;
            }
            WorkspaceCommands::Remove { workspace, force } => {
                workspace::remove(&workspace, force)?;
            }
        },
    }

    Ok(())
}
