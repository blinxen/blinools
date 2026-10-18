.SH CONFIGURATION
Sandbox commands read the global configuration, then ./blinools.toml.
Both files are optional. Use \fB\-c FILE\fR or \fB\-\-config FILE\fR to select
a different project file, which must exist. The global configuration still applies.
.PP
Project values override global values per key. See \fBblinools.toml\fR(5) for
options and share inheritance rules. Other commands do not read configuration files.
.SH FILES
.TP
\fI$XDG_CONFIG_HOME/blinools/blinools.toml\fR
This file contains global configuration. If XDG_CONFIG_HOME is unset or empty,
blinools uses ~/.config/blinools/blinools.toml.
.TP
\fI./blinools.toml\fR
This file contains project configuration in the current working directory.
Parent directories are not searched.
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
blinools searches these directories for helper programs whose binary paths
are not configured.
.TP
\fBCOMPLETE\fR
This selects the dynamic shell completion protocol. The registration script
from \fBblinools completions\fR sets this value.
