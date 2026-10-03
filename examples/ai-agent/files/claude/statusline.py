#!/usr/bin/env python3
# Claude Code status line — model | context | duration | rate limits (5h/7d)
# settings.json: { "statusLine": { "type": "command", "command": "~/.claude/statusline.py" } }
import sys
import json
import time

RESET = "\033[0m"
DIM = "\033[2m"
GREY = "\033[38;5;245m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
ORANGE = "\033[38;5;208m"
RED = "\033[31m"

status = json.load(sys.stdin)


def field(*path, default=None):
    """Walk a nested-dict path, returning default if any key is missing."""
    value = status
    for key in path:
        value = value.get(key) if isinstance(value, dict) else None
    return default if value is None else value


def as_percent(value):
    """'45.6' -> 46, None/'' -> None."""
    return None if value in (None, "") else round(float(value))


def usage_color(percent):
    if percent >= 90:
        return RED
    if percent >= 75:
        return ORANGE
    if percent >= 50:
        return YELLOW
    return GREEN


def progress_bar(percent, width):
    filled = max(0, min(percent * width // 100, width))
    return "█" * filled + "░" * (width - filled)


def reset_countdown(resets_at):
    """Epoch seconds -> ' ↻2h30m', blank if absent or already past."""
    if not resets_at:
        return ""
    seconds_left = int(resets_at) - int(time.time())
    if seconds_left <= 0:
        return ""
    hours, minutes = divmod(seconds_left // 60, 60)
    countdown = f"↻{hours}h{minutes:02d}m" if hours else f"↻{minutes}m"
    return f" {GREY}{countdown}{RESET}"


def metric(label, percent, width, resets_at=None):
    """Render 'label ██░░ NN% ↻reset' for one metric."""
    color = usage_color(percent)
    return f"{DIM}{label}{RESET} {color}{progress_bar(percent, width)}{RESET} {percent}%{RESET}{reset_countdown(resets_at)}"


def format_duration(total_seconds):
    hours, remainder = divmod(total_seconds, 3600)
    minutes, seconds = divmod(remainder, 60)
    if hours:
        return f"{hours}h {minutes}m"
    if minutes:
        return f"{minutes}m {seconds}s"
    return f"{seconds}s"


def format_window(size):
    if size >= 1_000_000:
        return "1M"
    return f"{size // 1000}k" if size else "?"


# ---- gather metrics ----
model_name = field("model", "display_name", default="?")

context_percent = as_percent(field("context_window", "used_percentage", default=0)) or 0
token_count = int(field("context_window", "total_input_tokens", default=0)) + int(
    field("context_window", "total_output_tokens", default=0)
)
window = format_window(int(field("context_window", "context_window_size", default=0)))

duration = format_duration(int(field("cost", "total_duration_ms", default=0)) // 1000)

five_hour_percent = as_percent(field("rate_limits", "five_hour", "used_percentage"))
five_hour_reset = field("rate_limits", "five_hour", "resets_at")
seven_day_percent = as_percent(field("rate_limits", "seven_day", "used_percentage"))
seven_day_reset = field("rate_limits", "seven_day", "resets_at")

# ---- assemble rate-limit section ----
rate_limits = []
if five_hour_percent is not None:
    rate_limits.append(metric("5h", five_hour_percent, 8, five_hour_reset))
if seven_day_percent is not None:
    rate_limits.append(metric("7d", seven_day_percent, 8, seven_day_reset))

rate_section = f"{DIM} │ {RESET}".join(rate_limits) if rate_limits else f"{DIM}n/a{RESET}"

# ---- render ----
print(
    f"{model_name}  "
    f"🧠 {metric('ctx', context_percent, 10)} "
    f"{DIM}({token_count} tokens / {window}){RESET}   "
    f"⏱ {GREY}{duration}{RESET}   "
    f"📊 {rate_section}",
    end="",
)
