# NAME

blinools-sandbox-prune - Delete all stopped sandboxes and their saved rootfs changes

# SYNOPSIS

**blinools sandbox prune** \[OPTIONS\]

# DESCRIPTION

Delete all stopped sandboxes and their saved rootfs changes.

Asks for confirmation before deleting saved state.

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

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox(1)](blinools-sandbox.md), [blinools.toml(5)](../configuration.md)
