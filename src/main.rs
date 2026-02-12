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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Hello { name } => {
            println!("Hello, {name}!");
        }
    }
}
