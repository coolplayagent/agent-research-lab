#!/bin/sh
set -eu
case "$1 $2" in
  'repo status') cat status.json ;;
  'repo update') printf 'update\n' >> updates.log; cp after.json status.json; printf '{}\n' ;;
  *) exit 9 ;;
esac
