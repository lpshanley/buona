mod config;
mod workspace;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "buona", version, about = "A CLI tool", arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Example subcommand
    Hello {
        /// Name to greet
        #[arg(short, long, default_value = "world")]
        name: String,
    },

    /// View or set up the global configuration
    Config {
        /// Print config as JSON
        #[arg(long)]
        json: bool,

        /// Launch interactive setup wizard
        #[arg(long)]
        setup: bool,
    },

    /// Manage workspaces
    #[command(alias = "ws", arg_required_else_help = true)]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    /// List all workspaces in the configured directory
    List,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Hello { name } => {
            println!("Hello, {name}!");
        }
        Commands::Config { json, setup } => {
            if setup {
                config::run_setup();
            } else {
                let cfg = config::load_config();
                if json {
                    config::print_json(&cfg);
                } else {
                    config::print_pretty(&cfg);
                }
            }
        }
        Commands::Workspace { command } => match command {
            WorkspaceCommands::List => {
                workspace::list();
            }
        },
    }
}
