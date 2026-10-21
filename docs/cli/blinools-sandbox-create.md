# NAME

blinools-sandbox-create - Create and start a sandbox

# SYNOPSIS

**blinools sandbox create** \[OPTIONS\] \[NAME\]

# DESCRIPTION

Create and start a sandbox.

The command stays attached to the console until the guest shuts down. It reuses a stopped sandbox's rootfs overlay unless --recreate is passed.

# OPTIONS

**-s**, **--share** *\<SHARES\>*  
Add a filesystem share.

This option can be repeated. Supported forms are PATH, PATH:rw, PATH:ro, NAME:PATH:rw and NAME:PATH:ro. Shares are read-write by default. Unnamed shares take their name from the directory. Paths are relative to the working directory and cannot contain colons. This option replaces all settings of a configured share with the same name.

**--recreate**  
Recreate the sandbox from the configured rootfs.

This discards the existing rootfs overlay and all changes stored in it.

**--delete-after-shutdown**  
Delete the sandbox after it shuts down

**--read-only**  
Mount all shares read-only, overriding the configuration

**-c**, **--config** *\<FILE\>*  
Use FILE as the project configuration.

This replaces ./blinools.toml. The global configuration is read first.

**--no-hidden**  
Disable hidden-path masking.

Access to these paths depends on read_only, read_only_paths, git_action and host permissions.

**--hidden-as-read-only**  
Expose hidden paths as read-only instead of masking them

**-v**, **--verbose**  
Increase logging verbosity

**-q**, **--quiet**  
Decrease logging verbosity

**-h**, **--help**  
Print help (see a summary with '-h')

\[*NAME*\]  
Specify the sandbox name.

This overrides the name from the configuration. Fails if a sandbox with this name is already running.

# EXAMPLES

      blinools sandbox create work
      blinools sandbox create work --share .
      blinools sandbox create work --share source:.:ro
      blinools sandbox create work --recreate --delete-after-shutdown

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox(1)](blinools-sandbox.md), [blinools.toml(5)](../configuration.md)
