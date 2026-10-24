#!/usr/bin/env bash
# dev-cleanup.sh: stop the zz-dev daemons the bench started (xps, alienware default dev socket; ubuntu /tmp/zzlead.sock)
for h in xps alienware; do ssh -n "$h" 'grep -q zz-lead ~/.local/bin/.zz-dev-owner 2>/dev/null && ~/.local/bin/zz-dev kill-server' >/dev/null 2>&1; done
ssh -n ubuntu '~/zz-lead/lz kill-server' >/dev/null 2>&1
true
