//! CLI entry point for the buona workspace manager.

mod config;
mod fsutil;
mod output;
mod path_value;
mod run;
mod self_update;
mod styles;
mod workspace;

use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "buona",
    version,
    about = "The Good CLI — Workspace Bliss – Build More, Fuss Less.",
    arg_required_else_help = true
)]
struct Cli {
    /// Select human-readable or machine-readable output
    #[arg(long, global = true, value_enum, default_value_t)]
    output: output::OutputFormat,

    /// Disable terminal colors, including colors requested from child commands
    #[arg(long, global = true)]
    no_color: bool,

    /// Never prompt; fail with a hint when explicit confirmation is required
    #[arg(long, global = true)]
    non_interactive: bool,

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

    /// Describe the current workspace, target, configuration, and command plans
    Inspect {
        /// Inspect a specific workspace target (`root` or package name)
        #[arg(short = 't', long = "target")]
        target: Option<String>,
    },

    /// Create a `buona.json` in the current directory
    Init {
        /// Force a specific build system (skips auto-detection)
        #[arg(long, value_enum)]
        system: Option<run::BuildSystem>,

        /// Overwrite an existing `buona.json`
        #[arg(long)]
        force: bool,
    },

    /// Run a command in the current context or explicit target(s)
    Run {
        /// Force a specific build system (overrides auto-detection and the
        /// global `system` in buona.json; per-command overrides still win)
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

    /// Add values to a list configuration field (e.g. default_packages)
    Add {
        /// Configuration key path (e.g. default_packages)
        key: String,

        /// Values to add
        #[arg(required = true)]
        values: Vec<String>,
    },

    /// Remove values from a list configuration field (e.g. default_packages)
    Remove {
        /// Configuration key path (e.g. default_packages)
        key: String,

        /// Values to remove
        #[arg(required = true)]
        values: Vec<String>,
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

        /// Confirm deletion without prompting
        #[arg(
            short = 'y',
            long = "yes",
            visible_alias = "force",
            visible_short_alias = 'f'
        )]
        yes: bool,
    },

    /// Rename a workspace (updates metadata and the directory name)
    Rename {
        /// Name or directory of the workspace to rename
        workspace: String,

        /// New name for the workspace
        new_name: String,

        /// Only update the metadata name; keep the directory name unchanged
        #[arg(long)]
        keep_directory: bool,
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

        /// Confirm removal without prompting
        #[arg(
            short = 'y',
            long = "yes",
            visible_alias = "force",
            visible_short_alias = 'f'
        )]
        yes: bool,
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

    /// Add values to a list workspace configuration field
    Add {
        /// Configuration key path
        key: String,

        /// Values to add
        #[arg(required = true)]
        values: Vec<String>,

        /// Workspace name or directory (defaults to detecting from the current directory)
        #[arg(short, long)]
        workspace: Option<String>,
    },

    /// Remove values from a list workspace configuration field
    Remove {
        /// Configuration key path
        key: String,

        /// Values to remove
        #[arg(required = true)]
        values: Vec<String>,

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

        /// Allow install when the release has no `.sha256` checksum asset
        #[arg(long)]
        force_insecure: bool,

        /// Install a specific version (e.g. v0.1.5 or 0.1.5)
        version: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let json_requested = output::json_requested(&args);
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = error.exit_code() as u8;
            if exit_code == 0 {
                let _ = error.print();
            } else if json_requested {
                output::print_error(
                    "usage",
                    &error.to_string(),
                    exit_code,
                    Some("Run `buona --help` for command usage."),
                    None,
                );
            } else {
                let _ = error.print();
            }
            return ExitCode::from(exit_code);
        }
    };

    output::configure(cli.output, cli.no_color, cli.non_interactive);

    match dispatch(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // RunError carries a specific exit code (forwarded child exit
            // codes, or the documented 65/68/69 config errors).
            if let Some(run_err) = e.downcast_ref::<run::RunError>() {
                let exit_code = run_err.exit_code().clamp(1, 255) as u8;
                if output::is_json() {
                    output::print_error(
                        run_err.code(),
                        &run_err.to_string(),
                        exit_code,
                        run_err.hint(),
                        None,
                    );
                } else {
                    eprintln!("error: {run_err}");
                }
                return ExitCode::from(exit_code);
            }
            // {:#} prints the anyhow context chain in a readable form.
            if output::is_json() {
                output::print_error("error", &format!("{e:#}"), 1, None, None);
            } else {
                eprintln!("error: {e:#}");
            }
            ExitCode::FAILURE
        }
    }
}

async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Config { command } => match command {
            ConfigCommands::Show { json } => {
                let cfg = config::load_config().await?;
                if json || output::is_json() {
                    config::print_json(&cfg)?;
                } else {
                    config::print_pretty(&cfg).await?;
                }
            }
            ConfigCommands::Setup => {
                config::run_setup().await?;
                output::print_success("config.setup", json!({}))?;
            }
            ConfigCommands::Set { key, value, json } => {
                config::set_value(&key, value.as_deref(), json).await?;
                output::print_success("config.set", json!({ "key": key }))?;
            }
            ConfigCommands::Get { key } => {
                config::get_value(&key).await?;
            }
            ConfigCommands::Unset { key } => {
                config::unset_value(&key).await?;
                output::print_success("config.unset", json!({ "key": key }))?;
            }
            ConfigCommands::Add { key, values } => {
                config::add_to_list(&key, &values).await?;
                output::print_success("config.add", json!({ "key": key, "values": values }))?;
            }
            ConfigCommands::Remove { key, values } => {
                config::remove_from_list(&key, &values).await?;
                output::print_success("config.remove", json!({ "key": key, "values": values }))?;
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
                    workspace::CreateOptions {
                        name: name.as_deref(),
                        packages: packages.as_deref(),
                        open_ws: open,
                        git_tracking,
                        no_defaults,
                        template_override: template.as_deref(),
                        no_template,
                        no_install,
                    },
                )
                .await?;
                output::print_success("workspace.create", json!({ "path": path, "name": name }))?;
            }
            WorkspaceCommands::Delete { workspace, yes } => {
                workspace::delete(&workspace, yes).await?;
                output::print_success("workspace.delete", json!({ "workspace": workspace }))?;
            }
            WorkspaceCommands::Rename {
                workspace,
                new_name,
                keep_directory,
            } => {
                workspace::rename(&workspace, &new_name, keep_directory).await?;
                output::print_success(
                    "workspace.rename",
                    json!({
                        "workspace": workspace,
                        "new_name": new_name,
                        "keep_directory": keep_directory,
                    }),
                )?;
            }
            WorkspaceCommands::Add {
                packages,
                workspace,
            } => {
                workspace::add(&packages, workspace.as_deref()).await?;
                output::print_success(
                    "workspace.add",
                    json!({ "workspace": workspace, "packages": packages }),
                )?;
            }
            WorkspaceCommands::Remove {
                packages,
                workspace,
                yes,
            } => {
                workspace::remove_packages(&packages, workspace.as_deref(), yes).await?;
                output::print_success(
                    "workspace.remove",
                    json!({ "workspace": workspace, "packages": packages }),
                )?;
            }
            WorkspaceCommands::Sync {
                packages,
                workspace,
                fetch,
            } => {
                workspace::sync(&packages, workspace.as_deref(), fetch).await?;
                output::print_success(
                    "workspace.sync",
                    json!({
                        "workspace": workspace,
                        "packages": packages,
                        "fetch_only": fetch,
                    }),
                )?;
            }
            WorkspaceCommands::Open { workspace } => {
                workspace::open(workspace.as_deref()).await?;
                output::print_success("workspace.open", json!({ "workspace": workspace }))?;
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
                    output::print_success(
                        "workspace.config.set",
                        json!({ "workspace": workspace, "key": key }),
                    )?;
                }
                WorkspaceConfigCommands::Get { key, workspace } => {
                    workspace::config_get(&key, workspace.as_deref()).await?;
                }
                WorkspaceConfigCommands::Unset { key, workspace } => {
                    workspace::config_unset(&key, workspace.as_deref()).await?;
                    output::print_success(
                        "workspace.config.unset",
                        json!({ "workspace": workspace, "key": key }),
                    )?;
                }
                WorkspaceConfigCommands::Add {
                    key,
                    values,
                    workspace,
                } => {
                    workspace::config_add(&key, &values, workspace.as_deref()).await?;
                    output::print_success(
                        "workspace.config.add",
                        json!({ "workspace": workspace, "key": key, "values": values }),
                    )?;
                }
                WorkspaceConfigCommands::Remove {
                    key,
                    values,
                    workspace,
                } => {
                    workspace::config_remove(&key, &values, workspace.as_deref()).await?;
                    output::print_success(
                        "workspace.config.remove",
                        json!({ "workspace": workspace, "key": key, "values": values }),
                    )?;
                }
            },
            WorkspaceCommands::Info { workspace, json } => {
                workspace::info(workspace.as_deref(), json || output::is_json()).await?;
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
                output::print_success(
                    "workspace.adopt",
                    json!({
                        "workspace": workspace,
                        "path": path,
                        "copy": copy,
                        "name": name,
                    }),
                )?;
            }
        },
        Commands::Detect { targets, recursive } => {
            run::detect(targets, recursive).await?;
        }
        Commands::Inspect { target } => {
            run::inspect(target).await?;
        }
        Commands::Init { system, force } => {
            run::init(run::InitOptions { system, force }).await?;
            output::print_success(
                "init",
                json!({ "system": system.map(|value| value.to_string()), "force": force }),
            )?;
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
            run::execute(options).await?;
        }
        Commands::Self_ { command } => match command {
            SelfCommands::Update {
                check,
                yes,
                force_insecure,
                version,
            } => {
                self_update::update(self_update::UpdateOptions {
                    check,
                    yes,
                    force_insecure,
                    version: version.clone(),
                })
                .await?;
                output::print_success(
                    "self.update",
                    json!({ "check": check, "version": version }),
                )?;
            }
        },
    }

    Ok(())
}
