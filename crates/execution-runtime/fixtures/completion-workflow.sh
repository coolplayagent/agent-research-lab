#!/bin/sh
set -eu

if [ "${1-}" = run ] && [ "${2-}" = status ]; then
    authority="${3%/*}/../authority.json"
    exec cat -- "$authority"
fi

printf '%s\n' '{"ok":true,"result":{}}'
