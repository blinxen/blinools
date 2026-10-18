# NAME

blinools-sandbox-shutdown - Stop a sandbox through its hypervisor

# SYNOPSIS

**blinools sandbox shutdown** \[OPTIONS\] \<NAME\>

# DESCRIPTION

Stop a sandbox through its hypervisor.

To let the guest operating system shut down cleanly, run poweroff inside the guest. Does nothing when the hypervisor control socket is unavailable.

# OPTIONS

**-f**, **--force**  
Ignore guest shutdown failures reported by Cloud Hypervisor.

The command still fails if the virtual machine monitor cannot shut down. This flag does not change QEMU shutdown.

**-c**, **--config** *\<FILE\>*  
Use FILE as the project configuration.

This replaces ./blinools.toml. The global configuration still applies.

**-v**, **--verbose**  
Increase logging verbosity

**-q**, **--quiet**  
Decrease logging verbosity

**-h**, **--help**  
Print help (see a summary with '-h')

\<*NAME*\>  
Specify the sandbox name

# EXAMPLES

      blinools sandbox shutdown work
      blinools sandbox shutdown work --force

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox(1)](blinools-sandbox.md), [blinools.toml(5)](../configuration.md)
