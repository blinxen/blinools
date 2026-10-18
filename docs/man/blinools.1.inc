.SH CONFIGURATION
blinools reads a global configuration file, then ./blinools.toml. Both are optional.
Use \fB\-c FILE\fR or \fB\-\-config FILE\fR to select a different project file.
See \fBblinools.toml\fR(5) for the options and for how the files are combined.
.SH FILES
.TP
\fI$XDG_CONFIG_HOME/blinools/blinools.toml\fR
This file contains global configuration. See \fBblinools.toml\fR(5).
.TP
\fI./blinools.toml\fR
This file contains project configuration. See \fBblinools.toml\fR(5).
.TP
\fI$XDG_STATE_HOME/blinools/\fR
This directory contains persistent sandbox state. If XDG_STATE_HOME is unset
or empty, blinools uses ~/.local/state/blinools/.
.TP
\fI$XDG_RUNTIME_DIR/blinools/\fR
This directory contains runtime sockets, locks and other sandbox files.
If XDG_RUNTIME_DIR is unset or empty, blinools uses /run/user/UID/blinools/.
.SH ENVIRONMENT
.TP
\fBXDG_CONFIG_HOME\fR
This sets the base directory for global configuration. An empty value uses
the fallback.
.TP
\fBXDG_STATE_HOME\fR
This sets the base directory for persistent state. An empty value uses the fallback.
.TP
\fBXDG_RUNTIME_DIR\fR
This sets the base directory for runtime files. An empty value uses the fallback.
.TP
\fBHOME\fR
blinools uses this value to locate the home directory for configuration and
state fallbacks.
.TP
\fBPATH\fR
blinools searches these directories for helper programs whose binary paths are not configured.
.TP
\fBCOMPLETE\fR
This selects the dynamic shell completion protocol.
The registration script from \fBblinools completions\fR sets this value.
.SH EXIT STATUS
.TP
\fB0\fR
The command completed successfully.
.TP
\fB1\fR
The command failed.
Possible causes include invalid configuration or a missing interactive terminal.
.TP
\fB2\fR
The command-line arguments were invalid.
.SH COPYRIGHT
blinools is licensed under GPL-3.0-or-later. See LICENSE for the full text.
