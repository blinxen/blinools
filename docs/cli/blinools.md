# NAME

blinools - Run blinxen's command-line utilities.

# SYNOPSIS

**blinools** \[OPTIONS\] \<COMMAND\>

# DESCRIPTION

Run blinxen's command-line utilities.

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

[blinools-worktree(1)](blinools-worktree.md)  
Common git worktree operations used during development

[blinools-sandbox(1)](blinools-sandbox.md)  
Create and manage Linux microVM sandboxes

[blinools-completions(1)](blinools-completions.md)  
Generate shell completion registration scripts

# EXAMPLES

      blinools worktree create
      blinools sandbox create
      blinools --config ./sandbox.toml sandbox create work
      blinools completions bash

# CONFIGURATION

Sandbox commands read a global configuration file, then ./blinools.toml. Both are optional. Use **-c FILE** or **--config FILE** to select a different project file. See [blinools.toml(5)](../configuration.md) for the options and for how the files are combined.

# FILES

*\$XDG_CONFIG_HOME/blinools/blinools.toml*  
This file contains global configuration. See [blinools.toml(5)](../configuration.md).

*./blinools.toml*  
This file contains project configuration. See [blinools.toml(5)](../configuration.md).

*\$XDG_STATE_HOME/blinools/*  
This directory contains persistent sandbox state. If XDG_STATE_HOME is unset or empty, blinools uses ~/.local/state/blinools/.

*\$XDG_RUNTIME_DIR/blinools/*  
This directory contains runtime sockets, locks and other sandbox files. If XDG_RUNTIME_DIR is unset or empty, blinools uses /run/user/UID/blinools/.

# ENVIRONMENT

**XDG_CONFIG_HOME**  
This sets the base directory for global configuration. An empty value uses the fallback.

**XDG_STATE_HOME**  
This sets the base directory for persistent state. An empty value uses the fallback.

**XDG_RUNTIME_DIR**  
This sets the base directory for runtime files. An empty value uses the fallback.

**HOME**  
blinools uses this value to locate the home directory for configuration and state fallbacks.

**PATH**  
blinools searches these directories for helper programs whose binary paths are not configured.

**COMPLETE**  
This selects the dynamic shell completion protocol. The registration script from **blinools completions** sets this value.

# EXIT STATUS

**0**  
The command completed successfully.

**1**  
The command failed. Possible causes include invalid configuration or a missing interactive terminal.

**2**  
The command-line arguments were invalid.

# COPYRIGHT

blinools is licensed under GPL-3.0-or-later. See LICENSE for the full text.

# SEE ALSO

[blinools-completions(1)](blinools-completions.md), [blinools-sandbox(1)](blinools-sandbox.md), [blinools-sandbox-attach-share(1)](blinools-sandbox-attach-share.md), [blinools-sandbox-create(1)](blinools-sandbox-create.md), [blinools-sandbox-delete(1)](blinools-sandbox-delete.md), [blinools-sandbox-detach-share(1)](blinools-sandbox-detach-share.md), [blinools-sandbox-prune(1)](blinools-sandbox-prune.md), [blinools-sandbox-ps(1)](blinools-sandbox-ps.md), [blinools-sandbox-shutdown(1)](blinools-sandbox-shutdown.md), [blinools-worktree(1)](blinools-worktree.md), [blinools-worktree-review(1)](blinools-worktree-review.md), [blinools-worktree-wip(1)](blinools-worktree-wip.md), [blinools.toml(5)](../configuration.md)
