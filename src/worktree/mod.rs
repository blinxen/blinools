pub mod config;

use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use anyhow::{Context, ensure};
use clap::Subcommand;

use crate::worktree::config::Config;

#[derive(Subcommand)]
pub enum Command {
    /// Create a WIP (work-in-progress) branch from HEAD and check it out in a new worktree.
    ///
    /// The branch is created as <type>-<task-number>-<branch-name>.
    /// If the branch already exists, it is not created again and the existing branch
    /// is checked out in the new worktree.
    /// The worktree is created at <wip_prefix>_<branch> under the repository root,
    /// where any '/' in the branch name is replaced by '-'.
    /// <wip_prefix> comes from the config.
    #[command(
        after_long_help = "Examples:\n  blinools worktree wip fix-login\n  blinools worktree wip fix-login --type bug --task-number 123"
    )]
    Wip {
        /// Branch name
        #[arg(value_parser = parse_branch_name)]
        branch_name: String,
        /// Ticket type
        #[arg(short = 't', long = "type")]
        branch_type: Option<String>,
        /// Task number (requires --type)
        #[arg(
            short = 'n',
            long = "task-number",
            requires = "branch_type",
            value_parser = parse_task_number
        )]
        task_number: Option<String>,
    },
    /// Check out an already existing branch in a new worktree to review it.
    ///
    /// Fails if the branch does not exist.
    /// The worktree is created at <review_prefix>_<branch> under the repository root,
    /// where any '/' in the branch name is replaced by '-'.
    /// <review_prefix> comes from the config.
    #[command(after_long_help = "Examples:\n  blinools worktree review fix-login")]
    Review {
        /// Branch name
        branch_name: String,
    },
}

pub fn handle(command: Command, config: &Config) -> Result<(), anyhow::Error> {
    let path = Path::new(".");
    match command {
        Command::Wip {
            branch_name,
            branch_type,
            task_number,
        } => wip(path, branch_name, branch_type, task_number, config),
        Command::Review { branch_name } => review(path, branch_name, config),
    }
}

fn wip(
    starting_path: &Path,
    branch_name: String,
    branch_type: Option<String>,
    task_number: Option<String>,
    config: &Config,
) -> Result<(), anyhow::Error> {
    let (repo, root) = open_main_repo(starting_path)?;

    let mut branch = String::new();
    if let Some(branch_type) = branch_type {
        ensure!(
            config.types.contains(&branch_type),
            "unknown branch type {branch_type:?}; available types: {}",
            config.types.join(", ")
        );

        branch.push_str(&branch_type);
        branch.push('-');
        if let Some(number) = task_number {
            branch.push_str(&number);
            branch.push('-');
        }
    }
    branch.push_str(&branch_name);
    validate_git_branch_name(&branch)?;

    let destination = worktree_destination(&root, &config.wip_prefix, &branch)?;
    let full_name = branch_ref_name(&branch)?;

    let created = if branch_exists(&repo, &branch)? {
        None
    } else {
        Some(
            repo.reference(
                full_name.clone(),
                repo.head_commit().context("reading HEAD commit")?.id,
                gix::refs::transaction::PreviousValue::MustNotExist,
                "blinools worktree",
            )
            .context("creating branch")?,
        )
    };

    if let Err(error) = add_worktree(&repo, full_name, &destination) {
        if let Some(reference) = created {
            reference
                .delete()
                .context("removing branch after worktree creation failed")?;
        }
        return Err(error);
    }

    if created.is_some() {
        println!("Created {branch} at {}", destination.display());
    } else {
        println!(
            "Branch {branch} already exists, checked it out at {}",
            destination.display()
        );
    }

    Ok(())
}

fn review(starting_path: &Path, branch_name: String, config: &Config) -> Result<(), anyhow::Error> {
    let (repo, root) = open_main_repo(starting_path)?;

    ensure!(
        branch_exists(&repo, &branch_name)?,
        "branch {branch_name} does not exist"
    );

    let destination = worktree_destination(&root, &config.review_prefix, &branch_name)?;
    add_worktree(&repo, branch_ref_name(&branch_name)?, &destination)?;
    println!("Checked out {branch_name} at {}", destination.display());

    Ok(())
}

fn open_main_repo(starting_path: &Path) -> Result<(gix::Repository, PathBuf), anyhow::Error> {
    let repo = gix::discover(starting_path)
        .context("discovering git repository")?
        .main_repo()
        .context("looking for main worktree")?;
    let root = std::path::absolute(
        repo.workdir()
            .context("repository has no working directory")?,
    )
    .context("making repository root absolute")?;
    Ok((repo, root))
}

fn worktree_destination(root: &Path, prefix: &str, branch: &str) -> Result<PathBuf, anyhow::Error> {
    let destination = root.join(format!("{prefix}_{}", branch.replace('/', "-")));
    ensure!(
        !destination.exists(),
        "worktree destination {} already exists",
        destination.display()
    );
    Ok(destination)
}

fn branch_ref_name(branch: &str) -> Result<gix::refs::FullName, anyhow::Error> {
    gix::refs::FullName::try_from(format!("refs/heads/{branch}"))
        .context("building branch reference name")
}

fn add_worktree(
    repo: &gix::Repository,
    branch: gix::refs::FullName,
    destination: &Path,
) -> Result<(), anyhow::Error> {
    repo.add_worktree(
        destination,
        gix::worktree::add::Head::Attached(branch),
        gix::progress::Discard,
        &AtomicBool::new(false),
    )
    .with_context(|| {
        format!(
            "creating worktree at {} (a branch that is already checked out in another worktree cannot be used)",
            destination.display()
        )
    })?;
    Ok(())
}

fn branch_exists(repo: &gix::Repository, name: &str) -> anyhow::Result<bool> {
    let full_name = format!("refs/heads/{name}");
    let found = repo
        .try_find_reference(full_name.as_str())
        .with_context(|| format!("looking up branch {name}"))?;
    Ok(found.is_some())
}

fn parse_branch_name(name: &str) -> Result<String, anyhow::Error> {
    ensure!(
        (5..=50).contains(&name.chars().count()),
        "branch name must have between 5 and 50 characters"
    );
    validate_git_branch_name(name)?;
    Ok(name.to_owned())
}

fn parse_task_number(number: &str) -> Result<String, anyhow::Error> {
    ensure!(
        !number.is_empty() && number.bytes().all(|c| c.is_ascii_digit()),
        "task number must contain only digits"
    );
    Ok(number.to_owned())
}

fn validate_git_branch_name(branch: &str) -> Result<(), anyhow::Error> {
    ensure!(
        !branch.starts_with('-'),
        "branch name must not start with a hyphen"
    );
    branch_ref_name(branch)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command as Process;

    use clap::Parser;
    use tempfile::TempDir;

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    fn test_config() -> Config {
        Config {
            types: vec!["bug".to_owned(), "feat".to_owned()],
            wip_prefix: "wip".to_owned(),
            review_prefix: "review".to_owned(),
        }
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Process::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .expect("failed to run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn git_branch_exists(dir: &Path, branch: &str) -> bool {
        Process::new("git")
            .arg("-C")
            .arg(dir)
            .args(["show-ref", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"))
            .status()
            .expect("failed to run git")
            .success()
    }

    fn init_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        git(dir.path(), &["commit", "--allow-empty", "-m", "initial"]);
        dir
    }

    fn checked_out_branch(worktree: &Path) -> String {
        git(worktree, &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    fn registered_worktrees(repo: &Path) -> String {
        git(repo, &["worktree", "list", "--porcelain"])
    }

    #[test]
    fn cli_parses_wip_with_type_and_task_number() {
        let cli = Cli::try_parse_from([
            "blinools",
            "wip",
            "fix-login",
            "--type",
            "bug",
            "--task-number",
            "123",
        ])
        .unwrap();
        match cli.command {
            Command::Wip {
                branch_name,
                branch_type,
                task_number,
            } => {
                assert_eq!(branch_name, "fix-login");
                assert_eq!(branch_type.as_deref(), Some("bug"));
                assert_eq!(task_number.as_deref(), Some("123"));
            }
            _ => panic!("expected wip"),
        }
    }

    #[test]
    fn cli_parses_wip_with_only_a_branch_name() {
        let cli = Cli::try_parse_from(["blinools", "wip", "fix-login"]).unwrap();
        match cli.command {
            Command::Wip {
                branch_type,
                task_number,
                ..
            } => {
                assert!(branch_type.is_none());
                assert!(task_number.is_none());
            }
            _ => panic!("expected wip"),
        }
    }

    #[test]
    fn cli_rejects_task_number_without_type() {
        assert!(Cli::try_parse_from(["blinools", "wip", "fix-login", "-n", "123"]).is_err());
    }

    #[test]
    fn cli_rejects_non_numeric_task_number() {
        assert!(
            Cli::try_parse_from(["blinools", "wip", "fix-login", "-t", "bug", "-n", "12a"])
                .is_err()
        );
    }

    #[test]
    fn cli_requires_a_branch_name() {
        assert!(Cli::try_parse_from(["blinools", "wip"]).is_err());
        assert!(Cli::try_parse_from(["blinools", "review"]).is_err());
    }

    #[test]
    fn cli_rejects_too_short_wip_branch_name() {
        assert!(Cli::try_parse_from(["blinools", "wip", "fix"]).is_err());
    }

    #[test]
    fn branch_name_length_limits() {
        assert!(parse_branch_name("abcd").is_err());
        assert!(parse_branch_name("abcde").is_ok());
        assert!(parse_branch_name(&"a".repeat(50)).is_ok());
        assert!(parse_branch_name(&"a".repeat(51)).is_err());
    }

    #[test]
    fn branch_name_rejects_invalid_git_names() {
        assert!(parse_branch_name("-fix-login").is_err());
        assert!(parse_branch_name("bad..name").is_err());
        assert!(parse_branch_name("has space").is_err());
        assert!(parse_branch_name("ends.lock").is_err());
    }

    #[test]
    fn branch_name_allows_slashes() {
        assert_eq!(parse_branch_name("feature/login").unwrap(), "feature/login");
    }

    #[test]
    fn task_number_accepts_only_digits() {
        assert_eq!(parse_task_number("123").unwrap(), "123");
        assert!(parse_task_number("").is_err());
        assert!(parse_task_number("12a").is_err());
        assert!(parse_task_number("-1").is_err());
        assert!(parse_task_number("1 2").is_err());
    }

    #[test]
    fn destination_replaces_slashes_with_hyphens() {
        let dir = tempfile::tempdir().unwrap();
        let destination = worktree_destination(dir.path(), "wip", "feature/login").unwrap();
        assert_eq!(destination, dir.path().join("wip_feature-login"));
    }

    #[test]
    fn destination_that_exists_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("wip_fix-login")).unwrap();
        assert!(worktree_destination(dir.path(), "wip", "fix-login").is_err());
    }

    #[test]
    fn wip_creates_branch_and_worktree() {
        let repo = init_repo();
        let root = repo.path();

        wip(root, "fix-login".into(), None, None, &test_config()).unwrap();

        assert!(git_branch_exists(root, "fix-login"));
        let worktree = root.join("wip_fix-login");
        assert!(worktree.is_dir());
        assert_eq!(checked_out_branch(&worktree), "fix-login");
        assert!(registered_worktrees(root).contains("refs/heads/fix-login"));
        assert_eq!(
            git(root, &["rev-parse", "fix-login"]),
            git(root, &["rev-parse", "HEAD"])
        );
    }

    #[test]
    fn wip_with_type_and_task_number_builds_the_branch_name() {
        let repo = init_repo();
        let root = repo.path();

        wip(
            root,
            "fix-login".into(),
            Some("bug".into()),
            Some("123".into()),
            &test_config(),
        )
        .unwrap();

        assert!(git_branch_exists(root, "bug-123-fix-login"));
        let worktree = root.join("wip_bug-123-fix-login");
        assert_eq!(checked_out_branch(&worktree), "bug-123-fix-login");
    }

    #[test]
    fn wip_with_type_only_omits_the_task_number() {
        let repo = init_repo();
        let root = repo.path();

        wip(
            root,
            "fix-login".into(),
            Some("feat".into()),
            None,
            &test_config(),
        )
        .unwrap();

        assert!(git_branch_exists(root, "feat-fix-login"));
        assert!(root.join("wip_feat-fix-login").is_dir());
    }

    #[test]
    fn wip_rejects_unknown_type_and_creates_nothing() {
        let repo = init_repo();
        let root = repo.path();

        let error = wip(
            root,
            "fix-login".into(),
            Some("nope".into()),
            None,
            &test_config(),
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("unknown branch type"));
        assert!(!git_branch_exists(root, "nope-fix-login"));
        assert!(!root.join("wip_nope-fix-login").exists());
    }

    #[test]
    fn wip_replaces_slashes_in_the_directory_name_only() {
        let repo = init_repo();
        let root = repo.path();

        wip(root, "feature/login".into(), None, None, &test_config()).unwrap();

        assert!(git_branch_exists(root, "feature/login"));
        let worktree = root.join("wip_feature-login");
        assert_eq!(checked_out_branch(&worktree), "feature/login");
    }

    #[test]
    fn wip_reuses_an_existing_branch_without_moving_it() {
        let repo = init_repo();
        let root = repo.path();
        git(root, &["branch", "fix-login"]);
        git(root, &["commit", "--allow-empty", "-m", "second"]);
        let tip_before = git(root, &["rev-parse", "fix-login"]);
        assert_ne!(tip_before, git(root, &["rev-parse", "HEAD"]));

        wip(root, "fix-login".into(), None, None, &test_config()).unwrap();

        assert_eq!(git(root, &["rev-parse", "fix-login"]), tip_before);
        let worktree = root.join("wip_fix-login");
        assert_eq!(checked_out_branch(&worktree), "fix-login");
    }

    #[test]
    fn wip_fails_when_the_destination_exists_and_creates_no_branch() {
        let repo = init_repo();
        let root = repo.path();
        std::fs::create_dir(root.join("wip_fix-login")).unwrap();

        let error = wip(root, "fix-login".into(), None, None, &test_config()).unwrap_err();

        assert!(format!("{error:#}").contains("already exists"));
        assert!(!git_branch_exists(root, "fix-login"));
    }

    #[test]
    fn wip_keeps_a_preexisting_branch_when_the_worktree_cannot_be_created() {
        let repo = init_repo();
        let root = repo.path();
        git(root, &["checkout", "-b", "fix-login"]);

        let result = wip(root, "fix-login".into(), None, None, &test_config());

        assert!(result.is_err());
        assert!(git_branch_exists(root, "fix-login"));
        assert!(!root.join("wip_fix-login").exists());
    }

    #[test]
    fn wip_works_when_started_from_a_subdirectory() {
        let repo = init_repo();
        let root = repo.path();
        let subdir = root.join("src");
        std::fs::create_dir(&subdir).unwrap();

        wip(&subdir, "fix-login".into(), None, None, &test_config()).unwrap();

        assert!(root.join("wip_fix-login").is_dir());
        assert!(!subdir.join("wip_fix-login").exists());
    }

    #[test]
    fn wip_works_when_started_from_a_linked_worktree() {
        let repo = init_repo();
        let root = repo.path();
        wip(root, "first-task".into(), None, None, &test_config()).unwrap();

        let linked = root.join("wip_first-task");
        wip(&linked, "second-task".into(), None, None, &test_config()).unwrap();

        assert!(root.join("wip_second-task").is_dir());
        assert!(!linked.join("wip_second-task").exists());
    }

    #[test]
    fn review_checks_out_an_existing_branch() {
        let repo = init_repo();
        let root = repo.path();
        git(root, &["branch", "fix-login"]);
        let tip_before = git(root, &["rev-parse", "fix-login"]);

        review(root, "fix-login".into(), &test_config()).unwrap();

        let worktree = root.join("review_fix-login");
        assert!(worktree.is_dir());
        assert_eq!(checked_out_branch(&worktree), "fix-login");
        assert!(registered_worktrees(root).contains("refs/heads/fix-login"));
        assert_eq!(git(root, &["rev-parse", "fix-login"]), tip_before);
    }

    #[test]
    fn review_fails_for_a_missing_branch_and_creates_nothing() {
        let repo = init_repo();
        let root = repo.path();

        let error = review(root, "fix-login".into(), &test_config()).unwrap_err();

        assert!(format!("{error:#}").contains("does not exist"));
        assert!(!git_branch_exists(root, "fix-login"));
        assert!(!root.join("review_fix-login").exists());
    }

    #[test]
    fn review_replaces_slashes_in_the_directory_name() {
        let repo = init_repo();
        let root = repo.path();
        git(root, &["branch", "feature/login"]);

        review(root, "feature/login".into(), &test_config()).unwrap();

        let worktree = root.join("review_feature-login");
        assert_eq!(checked_out_branch(&worktree), "feature/login");
    }

    #[test]
    fn review_fails_when_the_destination_exists() {
        let repo = init_repo();
        let root = repo.path();
        git(root, &["branch", "fix-login"]);
        std::fs::create_dir(root.join("review_fix-login")).unwrap();

        let error = review(root, "fix-login".into(), &test_config()).unwrap_err();

        assert!(format!("{error:#}").contains("already exists"));
    }

    #[test]
    fn review_keeps_the_branch_when_the_worktree_cannot_be_created() {
        let repo = init_repo();
        let root = repo.path();
        git(root, &["checkout", "-b", "fix-login"]);

        let result = review(root, "fix-login".into(), &test_config());

        assert!(result.is_err());
        assert!(git_branch_exists(root, "fix-login"));
    }

    #[test]
    fn branch_name_accepts_plain_single_component_names() {
        assert_eq!(parse_branch_name("fix-login").unwrap(), "fix-login");
        assert!(validate_git_branch_name("bug-123-fix-login").is_ok());
        assert!(validate_git_branch_name("feat-fix-login").is_ok());
    }
}
