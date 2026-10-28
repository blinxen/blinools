# NAME

blinools-worktree-wip - Create a WIP (work-in-progress) branch from HEAD and check it out in a new worktree

# SYNOPSIS

**blinools worktree wip** \[OPTIONS\] \<BRANCH_NAME\>

# DESCRIPTION

Create a WIP (work-in-progress) branch from HEAD and check it out in a new worktree.

The branch is created as \<type\>-\<task-number\>-\<branch-name\>. If the branch already exists, it is not created again and the existing branch is checked out in the new worktree. The worktree is created at \<wip_prefix\>\_\<branch\> under the repository root, where any '/' in the branch name is replaced by '-'. \<wip_prefix\> comes from the config.

# OPTIONS

**-t**, **--type** *\<BRANCH_TYPE\>*  
Ticket type

**-n**, **--task-number** *\<TASK_NUMBER\>*  
Task number (requires --type)

**-c**, **--config** *\<FILE\>*  
Use FILE as the project configuration.

This replaces ./blinools.toml. The global configuration is read first.

**-v**, **--verbose**  
Increase logging verbosity

**-q**, **--quiet**  
Decrease logging verbosity

**-h**, **--help**  
Print help (see a summary with '-h')

\<*BRANCH_NAME*\>  
Branch name

# EXAMPLES

      blinools worktree wip fix-login
      blinools worktree wip fix-login --type bug --task-number 123

# SEE ALSO

[blinools(1)](blinools.md), [blinools-worktree(1)](blinools-worktree.md), [blinools.toml(5)](../configuration.md)
