# NAME

blinools-worktree - Common git worktree operations used during development

# SYNOPSIS

**blinools worktree** \[OPTIONS\] \<COMMAND\>

# DESCRIPTION

Common git worktree operations used during development

# OPTIONS

**-c**, **--config** *\<FILE\>*  
Use FILE as the project configuration.

This replaces ./blinools.toml. The global configuration is read first.

**-v**, **--verbose**  
Increase logging verbosity

**-q**, **--quiet**  
Decrease logging verbosity

**-h**, **--help**  
Print help (see a summary with '-h')

# SUBCOMMANDS

[blinools-worktree-create(1)](blinools-worktree-create.md)  
Create a branch from HEAD and check it out in a new worktree

# EXAMPLES

      blinools worktree create
      blinools worktree create my-branch --branch-type task --task-number 123

# SEE ALSO

[blinools(1)](blinools.md), [blinools-worktree-create(1)](blinools-worktree-create.md), [blinools.toml(5)](../configuration.md)
