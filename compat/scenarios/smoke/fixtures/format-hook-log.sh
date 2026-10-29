#!/bin/sh
log="$1"
shift
printf '%s\n' "$*" >>"$log"
