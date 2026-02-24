//! CLI entry point for the buona workspace manager.

mod config;
mod path_value;
mod run;
mod self_update;
mod styles;
mod workspace;

use std::path::Path;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "buona",
    version,
    about = "The Good CLI — Workspace Bliss – Build More, Fuss Less.",
    arg_required_else_help = true
)]
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

    /// Print the auto-detected build system for the current context/target(s)
    Detect {
        /// Execute detection only for the provided target(s): `root` or package name.
        /// Can be repeated and runs in the provided order.
        #[arg(short = 't', long = "target")]
        targets: Vec<String>,

        /// Detect recursively across workspace root and all packages (alphabetical)
        #[arg(short, long)]
        recursive: bool,
    },

    /// Run a command in the current context or explicit target(s)
    Run {
        /// Force a specific build system (overrides auto-detection)
        #[arg(long, value_enum)]
        system: Option<run::BuildSystem>,

        /// Show the resolved command but don't execute it
        #[arg(long)]
        dry_run: bool,

        /// Print detailed resolution information
        #[arg(short, long)]
        verbose: bool,

        /// Execute only for the provided target(s): `root` or package name.
        /// Can be repeated and runs in the provided order.
        #[arg(short = 't', long = "target")]
        targets: Vec<String>,

        /// Execute recursively across workspace root and all packages (alphabetical)
        #[arg(short, long)]
        recursive: bool,

        /// Enable parallel execution for recursive or explicit-target runs
        #[arg(long)]
        parallel: bool,

        /// Max concurrent package tasks when parallel mode is enabled
        #[arg(long)]
        jobs: Option<usize>,

        /// Failure behavior for parallel runs
        #[arg(long, value_enum)]
        fail_policy: Option<run::FailPolicy>,

        /// The command to run (e.g. build, test, lint)
        command: String,

        /// Additional arguments passed through to the underlying tool (after --)
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Manage the buona binary itself
    #[command(name = "self", arg_required_else_help = true)]
    Self_ {
        #[command(subcommand)]
        command: SelfCommands,
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

    /// Set a global configuration value
    Set {
        /// Configuration key path (e.g. git.host)
        key: String,

        /// Value (omit for boolean keys to imply `true`)
        value: Option<String>,

        /// Parse value as JSON (for objects/arrays)
        #[arg(long)]
        json: bool,
    },

    /// Get a global configuration value
    Get {
        /// Configuration key path (e.g. git.host)
        key: String,
    },

    /// Unset a global configuration value
    Unset {
        /// Configuration key path (e.g. git.host)
        key: String,
    },
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

        /// Package specifier(s) to add after creation: name, org/name, or a full git URL
        #[arg(short = 'p', long = "package")]
        packages: Option<Vec<String>>,

        /// Open the workspace in the configured editor after creation
        #[arg(long)]
        open: bool,

        /// Git tracking mode for this workspace (overrides global default)
        #[arg(long, value_enum)]
        git_tracking: Option<config::GitTracking>,

        /// Skip default packages from global config
        #[arg(long)]
        no_defaults: bool,

        /// Path to a template directory (overrides global config workspace_template)
        #[arg(long)]
        template: Option<String>,

        /// Skip workspace template
        #[arg(long, conflicts_with = "template")]
        no_template: bool,

        /// Skip auto-running install after adding packages
        #[arg(long)]
        no_install: bool,
    },

    /// Delete a workspace
    Delete {
        /// Name or directory of the workspace to delete
        workspace: String,

        /// Skip the confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Add packages to a workspace
    Add {
        /// Package specifier(s): name, org/name, or a full git URL
        #[arg(short = 'p', long = "package", required = true)]
        packages: Vec<String>,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,
    },

    /// Remove packages from a workspace
    Remove {
        /// Package name(s) to remove
        #[arg(short = 'p', long = "package", required = true)]
        packages: Vec<String>,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,

        /// Skip the confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Pull latest changes for all packages and sync the workspace file
    Sync {
        /// Package name(s) to sync (defaults to all packages)
        #[arg(short = 'p', long = "package")]
        packages: Vec<String>,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,

        /// Only fetch (don't merge) — equivalent to git fetch instead of git pull
        #[arg(short, long)]
        fetch: bool,
    },

    /// Open workspace in the configured editor
    Open {
        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,
    },

    /// Get or set workspace-specific configuration values
    #[command(arg_required_else_help = true)]
    Config {
        #[command(subcommand)]
        command: WorkspaceConfigCommands,
    },

    /// Show detailed information about a workspace
    Info {
        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,

        /// Print info as JSON
        #[arg(long)]
        json: bool,
    },

    /// Adopt an existing local directory into the workspace
    Adopt {
        /// Path to the directory to adopt
        path: String,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,

        /// Copy the directory instead of moving it
        #[arg(long)]
        copy: bool,

        /// Override the package name (defaults to the directory name)
        #[arg(short, long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum WorkspaceConfigCommands {
    /// Set a workspace configuration value
    Set {
        /// Configuration key (e.g. mount-root)
        key: String,

        /// Optional configuration value (defaults depend on key)
        value: Option<String>,

        /// Parse value as JSON (for objects/arrays)
        #[arg(long)]
        json: bool,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,
    },

    /// Get a workspace configuration value
    Get {
        /// Configuration key (e.g. mount-root)
        key: String,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,
    },

    /// Unset a workspace configuration value
    Unset {
        /// Configuration key (e.g. mount-root)
        key: String,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,
    },
}

#[derive(Subcommand)]
enum SelfCommands {
    /// Check for and install updates
    Update {
        /// Only check for updates without installing
        #[arg(long)]
        check: bool,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Install a specific version (e.g. v0.1.5 or 0.1.5)
        version: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { command } => match command {
            ConfigCommands::Show { json } => {
                let cfg = config::load_config().await?;
                if json {
                    config::print_json(&cfg)?;
                } else {
                    config::print_pretty(&cfg).await?;
                }
            }
            ConfigCommands::Setup => {
                config::run_setup().await?;
            }
            ConfigCommands::Set { key, value, json } => {
                config::set_value(&key, value.as_deref(), json).await?;
            }
            ConfigCommands::Get { key } => {
                config::get_value(&key).await?;
            }
            ConfigCommands::Unset { key } => {
                config::unset_value(&key).await?;
            }
        },
        Commands::Workspace { command } => match command {
            WorkspaceCommands::List => {
                workspace::list().await?;
            }
            WorkspaceCommands::Create {
                path,
                name,
                packages,
                open,
                git_tracking,
                no_defaults,
                template,
                no_template,
                no_install,
            } => {
                workspace::create(
                    Path::new(&path),
                    name.as_deref(),
                    packages.as_deref(),
                    open,
                    git_tracking,
                    no_defaults,
                    template.as_deref(),
                    no_template,
                    no_install,
                )
                .await?;
            }
            WorkspaceCommands::Delete { workspace, force } => {
                workspace::delete(&workspace, force).await?;
            }
            WorkspaceCommands::Add {
                packages,
                workspace,
            } => {
                workspace::add(&packages, workspace.as_deref()).await?;
            }
            WorkspaceCommands::Remove {
                packages,
                workspace,
                force,
            } => {
                workspace::remove_packages(&packages, workspace.as_deref(), force).await?;
            }
            WorkspaceCommands::Sync {
                packages,
                workspace,
                fetch,
            } => {
                workspace::sync(&packages, workspace.as_deref(), fetch).await?;
            }
            WorkspaceCommands::Open { workspace } => {
                workspace::open(workspace.as_deref()).await?;
            }
            WorkspaceCommands::Config { command } => match command {
                WorkspaceConfigCommands::Set {
                    key,
                    value,
                    json,
                    workspace,
                } => {
                    workspace::config_set(&key, value.as_deref(), json, workspace.as_deref())
                        .await?;
                }
                WorkspaceConfigCommands::Get { key, workspace } => {
                    workspace::config_get(&key, workspace.as_deref()).await?;
                }
                WorkspaceConfigCommands::Unset { key, workspace } => {
                    workspace::config_unset(&key, workspace.as_deref()).await?;
                }
            },
            WorkspaceCommands::Info { workspace, json } => {
                workspace::info(workspace.as_deref(), json).await?;
            }
            WorkspaceCommands::Adopt {
                path,
                workspace,
                copy,
                name,
            } => {
                workspace::adopt(
                    Path::new(&path),
                    workspace.as_deref(),
                    copy,
                    name.as_deref(),
                )
                .await?;
            }
        },
        Commands::Detect { targets, recursive } => {
            if let Err(e) = run::detect(targets, recursive).await {
                if let Some(run_err) = e.downcast_ref::<run::RunError>() {
                    run_err.exit();
                }
                eprintln!("error: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::Run {
            system,
            dry_run,
            verbose,
            targets,
            recursive,
            parallel,
            jobs,
            fail_policy,
            command,
            args,
        } => {
            let options = run::RunOptions {
                system,
                dry_run,
                verbose,
                targets,
                recursive,
                parallel,
                jobs,
                fail_policy,
                command,
                args,
            };
            if let Err(e) = run::execute(options).await {
                if let Some(run_err) = e.downcast_ref::<run::RunError>() {
                    run_err.exit();
                }
                eprintln!("error: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::Self_ { command } => match command {
            SelfCommands::Update {
                check,
                yes,
                version,
            } => {
                self_update::update(self_update::UpdateOptions {
                    check,
                    yes,
                    version,
                })
                .await?;
            }
        },
    }

    Ok(())
}
