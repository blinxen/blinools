# NAME

blinools-worktree-create - Create a branch from HEAD and check it out in a new worktree

# SYNOPSIS

**blinools worktree create** \[OPTIONS\] \[BRANCH_NAME\]

# DESCRIPTION

Create a branch from HEAD and check it out in a new worktree.

With no arguments, this will prompt for an optional ticket number, its type and a branch name. Otherwise, the branch name is required and no prompts appear. The branch is created as \<type\>-\<ticket-number\>-\<branch-name\>.

The worktree is created at wip-pr-\<branch-name\> under the repository root.

# OPTIONS

**-t**, **--type** *\<BRANCH_TYPE\>*  
Ticket type

**-n**, **--task-number** *\<TASK_NUMBER\>*  
Task number

**-c**, **--config** *\<FILE\>*  
Use FILE as the project configuration.

This replaces ./blinools.toml. The global configuration is read first.

**-v**, **--verbose**  
Increase logging verbosity

**-q**, **--quiet**  
Decrease logging verbosity

**-h**, **--help**  
Print help (see a summary with '-h')

\[*BRANCH_NAME*\]  
Branch name

# EXAMPLES

      blinools worktree create
      blinools worktree create fix-login
      blinools worktree create fix-login --type bug --task-number 123

# SEE ALSO

[blinools(1)](blinools.md), [blinools-worktree(1)](blinools-worktree.md), [blinools.toml(5)](../configuration.md)
