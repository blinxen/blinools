mod config;
#[cfg(feature = "man")]
mod man;
mod sandbox;
mod worktree;

use std::io::IsTerminal;

use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{CompleteEnv, Shell, env::Shells};

#[derive(Parser)]
#[command(
    name = "blinools",
    about = "Run blinxen's command-line utilities.",
    after_long_help = "Examples:\n  blinools worktree create\n  blinools sandbox create\n  blinools --config ./sandbox.toml sandbox create work\n  blinools completions bash"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Use FILE as the project configuration.
    ///
    /// This replaces ./blinools.toml. The global configuration is read first.
    #[arg(short = 'c', long = "config", global = true, value_name = "FILE")]
    config_file: Option<String>,

    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

#[derive(Subcommand)]
enum Commands {
    /// Common git worktree operations used during development.
    #[command(
        after_long_help = "Examples:\n  blinools worktree create\n  blinools worktree create my-branch --branch-type task --task-number 123"
    )]
    Worktree {
        #[command(subcommand)]
        command: worktree::Command,
    },

    /// Create and manage Linux microVM sandboxes.
    ///
    /// All sandbox commands require an interactive terminal on standard input.
    /// To create a sandbox, set kernel, rootfs, memory_mb and cpus in the configuration.
    #[command(
        after_long_help = "Examples:\n  blinools sandbox ps\n  blinools sandbox create work\n  blinools sandbox shutdown work"
    )]
    Sandbox {
        #[command(subcommand)]
        command: sandbox::Command,
    },

    /// Generate shell completion registration scripts.
    ///
    /// Source the output in the selected shell to enable dynamic completion.
    #[command(
        after_long_help = "Examples:\n  source <(blinools completions bash)\n  source <(blinools completions zsh)\n  blinools completions fish | source"
    )]
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },

    #[cfg(feature = "man")]
    #[command(hide = true)]
    Man { directory: std::path::PathBuf },
}

fn main() -> Result<(), anyhow::Error> {
    CompleteEnv::with_factory(Cli::command).complete();

    let args = Cli::parse();
    env_logger::Builder::new()
        .filter_level(args.verbosity.into())
        .init();

    match args.command {
        Commands::Worktree { command } => {
            let config = config::parse_config(args.config_file.as_ref())?;
            worktree::handle(command, &config.worktree)?;
        }
        Commands::Sandbox { command } => {
            if !std::io::stdin().is_terminal() {
                eprintln!("This command requires an interactive terminal session");
                std::process::exit(1);
            }
            let config = config::parse_config(args.config_file.as_ref())?;
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
        #[cfg(feature = "man")]
        Commands::Man { directory } => man::generate(Cli::command(), &directory)?,
    };

    Ok(())
}
