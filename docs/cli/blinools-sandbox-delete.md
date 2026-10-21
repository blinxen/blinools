# NAME

blinools-sandbox-delete - Delete a sandbox and all of its saved rootfs changes

# SYNOPSIS

**blinools sandbox delete** \[OPTIONS\] \<NAME\>

# DESCRIPTION

Delete a sandbox and all of its saved rootfs changes.

Removes only runtime files and the rootfs overlay. The base image and files in host shares are not touched. Deleting a running sandbox fails unless --force is supplied.

# OPTIONS

**-f**, **--force**  
Shut down a running sandbox before deleting it

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
Specify the sandbox name

# EXAMPLES

      blinools sandbox delete work
      blinools sandbox delete work --force

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox(1)](blinools-sandbox.md), [blinools.toml(5)](../configuration.md)
