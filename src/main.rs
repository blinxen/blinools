mod config;
mod sandbox;
mod wip_pr;

use std::io::IsTerminal;

use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{CompleteEnv, Shell, env::Shells};

#[derive(Parser)]
#[command(name = "blinools", about = "Common utilities blinxen uses")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short = 'c', long = "config", global = true)]
    config_file: Option<String>,
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

    /// Manage sanboxes
    Sandbox {
        #[command(subcommand)]
        command: sandbox::Command,
    },

    /// Generate shell completion scripts
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<(), anyhow::Error> {
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    match cli.command {
        Commands::WipPr {
            branch_name,
            branch_type,
            task_number,
        } => wip_pr::create(&branch_name, branch_type.as_deref(), task_number.as_deref())?,
        Commands::Sandbox { command } => {
            if !std::io::stdin().is_terminal() {
                eprintln!("This command requires a interactive terminal session");
                std::process::exit(1);
            }
            let config = config::parse_config(cli.config_file.as_ref())?;
            config::setup_dirs()?;
            sandbox::handle(command, config.sandbox)?
        }
        Commands::Completions { shell } => {
            let shell = shell.to_string();
            let shells = Shells::builtins();
            let completer = shells
                .completer(&shell)
                .with_context(|| format!("no completion support for `{shell}`"))?;
            let name = Cli::command().get_name().to_string();

            completer
                .write_registration("COMPLETE", &name, &name, &name, &mut std::io::stdout())
                .context("generating the completion script")?;
        }
    };

    Ok(())
}
