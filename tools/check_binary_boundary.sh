#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
case "$(uname -s)" in
Linux) boundary_os=Linux ;;
Darwin) boundary_os=macOS ;;
*)
  echo 'The native import-table boundary has Linux/macOS readers only.' >&2
  exit 1
  ;;
esac
bin="$(python3 tools/ci_inputs.py --binary)"
# Whole symbols and the reader's measured floor are required for both original checks.
case "$boundary_os" in
Linux) undefined() { nm -D --undefined-only "$1" | awk '{print $NF}' | sed 's/@.*//' | sort -u; } ;;
macOS) undefined() { nm -u "$1" | sed 's/^_//' | sort -u; } ;;
*)
  echo "no reader for $boundary_os" >&2
  exit 1
  ;;
esac

test -f "$bin" || {
  echo "no binary at $bin" >&2
  exit 1
}
count=$(undefined "$bin" | grep -c . || true)
if [ "$count" -lt 10 ]; then
  echo "read only $count undefined symbols from $bin; the reader is not working" >&2
  exit 1
fi
echo "read $count undefined symbols"

net=$(undefined "$bin" | grep -xE 'socket|socketpair|connect|bind|listen|accept|accept4|sendto|sendmsg|recvfrom|recvmsg|getaddrinfo|gethostbyname|gethostbyname2|res_query|res_search' || true)
echo "network imports: ${net:-(none)}"

if [ -n "$net" ]; then
  echo "this build imports network symbols:" >&2
  echo "$net" >&2
  exit 1
fi
echo "no network symbol is imported"

test -f "$bin" || {
  echo "no binary at $bin" >&2
  exit 1
}
count=$(undefined "$bin" | grep -c . || true)
if [ "$count" -lt 10 ]; then
  echo "read only $count undefined symbols from $bin; the reader is not working" >&2
  exit 1
fi

spawn=$(undefined "$bin" | grep -xE 'execve|execv|execvp|execvpe|fork|vfork|posix_spawn|posix_spawnp' || true)
declared=$("$bin" provider-info | python3 -c 'import json,sys; print("yes" if "launch" in json.load(sys.stdin)["supported_commands"] else "no")')
echo "launch declared: $declared"
echo "spawn imports: ${spawn:-(none)}"

if [ "$declared" = "no" ] && [ -n "$spawn" ]; then
  echo "no launch is declared and yet this build can spawn" >&2
  exit 1
fi
if [ "$declared" = "yes" ] && [ -z "$spawn" ]; then
  echo "launch is declared and this build imports nothing that could" >&2
  exit 1
fi
echo "the import table agrees with the declaration"
