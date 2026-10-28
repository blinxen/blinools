# NAME

blinools-worktree-review - Check out an already existing branch in a new worktree to review it

# SYNOPSIS

**blinools worktree review** \[OPTIONS\] \<BRANCH_NAME\>

# DESCRIPTION

Check out an already existing branch in a new worktree to review it.

Fails if the branch does not exist. The worktree is created at \<review_prefix\>\_\<branch\> under the repository root, where any '/' in the branch name is replaced by '-'. \<review_prefix\> comes from the config.

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

\<*BRANCH_NAME*\>  
Branch name

# EXAMPLES

      blinools worktree review fix-login

# SEE ALSO

[blinools(1)](blinools.md), [blinools-worktree(1)](blinools-worktree.md), [blinools.toml(5)](../configuration.md)
