#!/usr/bin/env bash
set -eu
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
level="$(printf '%s' "${REVAYAT_LOG_LEVEL:-INFO}" | tr '[:lower:]' '[:upper:]')"
case "$level" in
    DEBUG) threshold=10;; INFO) threshold=20;; WARNING) threshold=30;;
    ERROR) threshold=40;; CRITICAL) threshold=50;;
    *) printf '[%s UTC] [ERROR] [install-bootstrap] Invalid REVAYAT_LOG_LEVEL.\n' "$(date -u '+%Y-%m-%d %H:%M:%S')" >&2; exit 2;;
esac
log_ready=0
log_dir="${REVAYAT_LOG_DIR:-$script_dir/../skills/revayat-subtitle/logs}"
stamp="$(date -u '+%Y-%m-%d_%H-%M-%S')"
if mkdir -p -- "$log_dir" 2>/dev/null; then
    for ((number=0; number<1000; number++)); do
        suffix=""; if ((number)); then suffix="_$number"; fi
        if (set -C; : > "$log_dir/install-bootstrap_${stamp}_UTC$suffix.log") 2>/dev/null; then
            exec 3>> "$log_dir/install-bootstrap_${stamp}_UTC$suffix.log"
            log_ready=1
            break
        fi
    done
fi
log_event() {
    severity="$1"; value="$2"; message="$3"
    if ((value < threshold)); then return; fi
    line="[$(date -u '+%Y-%m-%d %H:%M:%S') UTC] [$severity] [install-bootstrap] $message"
    if ((log_ready)); then
        if ! printf '%s\n' "$line" >&3; then
            log_ready=0
            printf '[%s UTC] [WARNING] [install-bootstrap] File logging failed; using stderr.\n' "$(date -u '+%Y-%m-%d %H:%M:%S')" >&2
        fi
    fi
    if ((log_ready == 0 || value >= 30)); then printf '%s\n' "$line" >&2; fi
}
if ((log_ready == 0)); then
    printf '[%s UTC] [WARNING] [install-bootstrap] File logging unavailable; using stderr.\n' "$(date -u '+%Y-%m-%d %H:%M:%S')" >&2
fi
trap 'if ((log_ready)); then exec 3>&-; fi' EXIT
log_event INFO 20 'Checking Python prerequisite.'
for interpreter in python3 python; do
    if command -v "$interpreter" >/dev/null 2>&1 &&
       "$interpreter" -B -c 'import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)' 2>/dev/null; then
        log_event DEBUG 10 'Compatible interpreter found; arguments are forwarded without logging values.'
        set +e
        "$interpreter" -B -X utf8 "$script_dir/install.py" "$@"
        result=$?
        set -e
        if ((result == 0)); then log_event INFO 20 'Installer completed exit=0.'
        else log_event ERROR 40 "Installer completed exit=$result."; fi
        exit "$result"
    fi
done
log_event ERROR 40 'Python 3.10+ is required. Install Python, then run this installer again.'
exit 2
