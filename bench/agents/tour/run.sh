#!/usr/bin/env bash
S=$(cd "$(dirname "$0")" && pwd)
OUT=$S/out TMO=${TMO:-900} exec "$S/../mentions/run-one.sh" "${1:?host: claude or codex}" tour "${2:-1}"
