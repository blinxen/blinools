pub mod config;

use std::{path::Path, sync::atomic::AtomicBool};

use anyhow::{Context, ensure};
use clap::Subcommand;
use dialoguer::{Input, Select};

use crate::worktree::config::Config;

#[derive(Subcommand)]
pub enum Command {
    /// Create a branch from HEAD and check it out in a new worktree.
    ///
    /// With no arguments, this will prompt for an optional ticket number, its type and
    /// a branch name. Otherwise, the branch name is required and no prompts appear.
    /// The branch is created as <type>-<ticket-number>-<branch-name>.
    ///
    /// The worktree is created at wip-pr-<branch-name> under the repository root.
    #[command(
        after_long_help = "Examples:\n  blinools worktree create\n  blinools worktree create fix-login\n  blinools worktree create fix-login --type bug --task-number 123"
    )]
    Create {
        /// Branch name
        #[arg(value_parser = parse_branch_name)]
        branch_name: Option<String>,
        /// Ticket type
        #[arg(short = 't', long = "type", requires = "branch_name")]
        branch_type: Option<String>,
        /// Task number
        #[arg(short = 'n', long = "task-number", requires = "branch_name", value_parser = parse_task_number)]
        task_number: Option<String>,
    },
}

pub fn handle(command: Command, config: &Config) -> Result<(), anyhow::Error> {
    match command {
        Command::Create {
            branch_name,
            branch_type,
            task_number,
        } => create(branch_name, branch_type, task_number, config),
    }
}

fn create(
    branch_name: Option<String>,
    branch_type: Option<String>,
    task_number: Option<String>,
    config: &Config,
) -> Result<(), anyhow::Error> {
    let repo = gix::discover(".")
        .context("discovering git repository")?
        .main_repo()
        .context("looking for main worktree")?;
    let root = repo
        .workdir()
        .context("repository has no working directory")?;

    let (name, branch_type, task_number) = match branch_name {
        Some(name) => (name, branch_type, task_number),
        None => prompt(config)?,
    };
    let branch = build_branch_name(
        &name,
        branch_type.as_deref(),
        task_number.as_deref(),
        config,
    )?;

    let destination = root.join(format!("wip-pr-{name}"));
    create_worktree(&repo, &branch, &destination)?;

    println!("Created {branch} at {}", destination.display());

    Ok(())
}

fn prompt(config: &Config) -> Result<(String, Option<String>, Option<String>), anyhow::Error> {
    let ticket: String = Input::new()
        .with_prompt("Ticket number (optional)")
        .allow_empty(true)
        .validate_with(|value: &String| {
            if value.is_empty() {
                Ok(())
            } else {
                parse_task_number(value).map(|_| ())
            }
        })
        .interact_text()?;
    let branch_type = if !ticket.is_empty() && !config.types.is_empty() {
        let selection = Select::new()
            .with_prompt("Type")
            .items(&config.types)
            .default(0)
            .interact()?;
        Some(config.types[selection].clone())
    } else {
        None
    };
    let name = Input::new()
        .with_prompt("Branch name")
        .validate_with(|value: &String| parse_branch_name(value).map(|_| ()))
        .interact_text()?;
    Ok((name, branch_type, (!ticket.is_empty()).then_some(ticket)))
}

fn parse_branch_name(name: &str) -> Result<String, anyhow::Error> {
    ensure!(
        (5..=50).contains(&name.chars().count()),
        "branch name must have between 5 and 50 characters"
    );
    validate_git_branch_name(name)?;
    Ok(name.to_owned())
}

fn validate_git_branch_name(branch: &str) -> Result<(), anyhow::Error> {
    ensure!(
        !branch.starts_with('-'),
        "branch name must not start with a hyphen"
    );
    gix::validate::reference::branch_name(branch.into())
        .context("invalid Git branch name")?;
    Ok(())
}

fn parse_task_number(number: &str) -> Result<String, anyhow::Error> {
    ensure!(
        !number.is_empty() && number.bytes().all(|c| c.is_ascii_digit()),
        "task number must contain only digits"
    );
    Ok(number.to_owned())
}

fn build_branch_name(
    name: &str,
    branch_type: Option<&str>,
    number: Option<&str>,
    config: &Config,
) -> Result<String, anyhow::Error> {
    parse_branch_name(name)?;
    if let Some(branch_type) = branch_type {
        ensure!(
            config.types.iter().any(|value| value == branch_type),
            "unknown PR type {branch_type:?}; available types: {}",
            config.types.join(", ")
        );
    }
    if let Some(number) = number {
        parse_task_number(number)?;
    }
    let branch = format!("refs/heads/{}",
        branch_type
            .into_iter()
            .chain(number)
            .chain(Some(name))
            .collect::<Vec<_>>()
            .join("-")
    );
    validate_git_branch_name(&branch)?;
    Ok(branch)
}

fn create_worktree(
    repo: &gix::Repository,
    branch: &str,
    destination: &Path,
) -> Result<(), anyhow::Error> {
    ensure!(
        !destination.exists(),
        "worktree destination already exists"
    );
    let reference = repo
        .reference(
            branch,
            repo.head_commit().context("reading HEAD commit")?.id,
            gix::refs::transaction::PreviousValue::MustNotExist,
            "blinools worktree create",
        )
        .context("creating branch")?;
    if let Err(error) = repo.add_worktree(
        destination,
        gix::worktree::add::Head::Attached(reference.name().to_owned()),
        gix::progress::Discard,
        &AtomicBool::new(false),
    ) {
        reference
            .delete()
            .context("removing branch after worktree creation failed")?;
        return Err(error).context("creating worktree");
    }
    Ok(())
}
