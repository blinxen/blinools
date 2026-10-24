# NAME

blinools-sandbox-attach-share - Attach a share to a running sandbox

# SYNOPSIS

**blinools sandbox attach-share** \[OPTIONS\] \<NAME\> \<SHARE\>

# DESCRIPTION

Attach a share to a running sandbox.

The share is not mounted automatically. You will be given the required command to run in the sandbox to mount the share.

# OPTIONS

**--read-only-path** *\<PATH\>*  
Expose a path inside the share as read-only.

This option can be repeated. It has no effect on a read-only share.

**--hidden-path** *\<PATH\>*  
Mask a path inside the share.

This option can be repeated. Directories appear empty and files are replaced by an empty file.

**--git-action** *\<GIT_ACTION\>*  
Specify how the sandbox sees \`.git\` directories in the share.

This overrides the git_action from the configuration for this share.  

  
*Possible values:*

- none: Leave \`.git\` directories untouched

- read-only: Expose \`.git\` directories read-only

- hide: Hide \`.git\` directories from the sandbox

**-c**, **--config** *\<FILE\>*  
Use FILE as the project configuration.

This replaces ./blinools.toml. The global configuration is read first.

**-v**, **--verbose**  
Increase logging verbosity

**-q**, **--quiet**  
Decrease logging verbosity

**-h**, **--help**  
Print help (see a summary with '-h')

\<*NAME*\>  
Specify the name of the running sandbox

\<*SHARE*\>  
Specify the path to the share.

Supported forms are PATH, PATH:rw, PATH:ro, NAME:PATH:rw and NAME:PATH:ro. The share is read-write by default. An unnamed share takes its name from the directory. The name is the virtio-fs tag the guest mounts. Paths are relative to the working directory and cannot contain colons.

# EXAMPLES

      blinools sandbox attach-share work ./data
      blinools sandbox attach-share work notes:./notes:ro
      blinools sandbox attach-share work ./src --hidden-path ./src/.env --git-action hide
      blinools sandbox attach-share work ./src --read-only-path ./src/vendor

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox(1)](blinools-sandbox.md), [blinools.toml(5)](../configuration.md)
