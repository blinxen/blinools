# NAME

blinools-sandbox-detach-share - Detach a share from a running sandbox

# SYNOPSIS

**blinools sandbox detach-share** \[OPTIONS\] \<NAME\> \<SHARE\>

# DESCRIPTION

Detach a share from a running sandbox

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

\<*NAME*\>  
Specify the name of the running sandbox

\<*SHARE*\>  
Specify the name of the share

# EXAMPLES

      blinools sandbox detach-share work data

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox(1)](blinools-sandbox.md), [blinools.toml(5)](../configuration.md)
