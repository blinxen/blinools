mod config;
mod sandbox;
mod wip_pr;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "blinools", about = "Common utilities blinxen uses")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short = 'c', long = "config", default_value = "./blinools.toml")]
    config_file: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Creates a worktree in the current directory.
    ///
    /// The branch naming scheme is <BRANCH_TYPE>-<TASK_NUMBER>-<BRANCH_NAME>.
    /// BRANCH_TYPE and TASK_NUMBER are optional and can be omitted.
    /// The worktree will have the name wip_pr-<BRANCH_NAME>.
    /// If the branch already exists then it will be reused
    WipPr {
        /// The branch name
        branch_name: String,

        /// Optional branch type
        #[arg(short = 't', long = "branch-type")]
        branch_type: Option<String>,

        /// Optional task number
        #[arg(short = 'n', long = "task-number")]
        task_number: Option<String>,
    },

    ///
    /// Manage sanboxes
    Sandbox {
        #[command(subcommand)]
        command: sandbox::Command,
    },
}

fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    let config = config::parse_config(&cli.config_file)?;
    config::setup_dirs()?;

    match cli.command {
        Commands::WipPr {
            branch_name,
            branch_type,
            task_number,
        } => wip_pr::create(&branch_name, branch_type.as_deref(), task_number.as_deref())?,
        Commands::Sandbox { command } => {
            if let Some(config) = config
                && let Some(sandbox_config) = config.sandbox
            {
                sandbox::handle(command, sandbox_config)?
            } else {
                eprintln!("Could not find sandbox configuration");
                std::process::exit(1);
            }
        }
    };

    Ok(())
}
