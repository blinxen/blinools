# NAME

blinools-completions - Generate shell completion registration scripts

# SYNOPSIS

**blinools completions** \[OPTIONS\] \<SHELL\>

# DESCRIPTION

Generate shell completion registration scripts.

Source the output in the selected shell to enable dynamic completion.

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

\<*SHELL*\>  
  
*Possible values:*

- bash

- elvish

- fish

- powershell

- zsh

# EXAMPLES

      source <(blinools completions bash)
      source <(blinools completions zsh)
      blinools completions fish | source

# SEE ALSO

[blinools(1)](blinools.md)
