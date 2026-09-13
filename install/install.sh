#!/usr/bin/env bash
set -eu
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
for interpreter in python3 python; do
    if command -v "$interpreter" >/dev/null 2>&1 &&
       "$interpreter" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)' 2>/dev/null; then
        exec "$interpreter" -X utf8 "$script_dir/install.py" "$@"
    fi
done
printf '%s\n' 'Python 3.10+ is required. Install Python, then run this installer again.' >&2
exit 2
