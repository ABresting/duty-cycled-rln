# Shared shell utilities for the testbed. Source this file.
#   . "$(dirname "$0")/../lib/common.sh"
#
# Provides leveled logging (info/warn/error/die) on stderr and small helpers.

_log() { printf '%s  %-5s %s\n' "$(date +%H:%M:%S)" "$1" "${*:2}" >&2; }
info()  { _log INFO  "$@"; }
warn()  { _log WARN  "$@"; }
error() { _log ERROR "$@"; }
die()   { error "$@"; exit 1; }

# require <cmd>...: fail unless every named command is on PATH.
require() {
  local c
  for c in "$@"; do command -v "$c" >/dev/null 2>&1 || die "required command not found: $c"; done
}
