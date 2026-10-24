#!/bin/bash
# killtree.sh PANE: kill everything under a lead pane's process the way a dead host would, keeping the pane
kt() { for c in $(pgrep -P "$1"); do kt "$c"; done; kill -9 "$1" 2>/dev/null; }
pid=$(zz display-message -p -t "$1" '#{pane_pid}')
kt "$pid"
true
