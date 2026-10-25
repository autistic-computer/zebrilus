#!/usr/bin/env bash
# dev-setup.sh BINARY_HOST: put the zz build under test on the fleet without touching the installed zz.
# The dev-identity zz_cli built in ~/zz-driver-build on BINARY_HOST becomes ~/.local/bin/zz-dev on xps and
# alienware (dev clients run zz-dev there), and ~/zz-driver/zz-dev on ubuntu behind ~/zz-driver/lz, which
# pins its own socket, config and data so the user's zz-dev setup stays as it was.
set -euo pipefail
src=${1:-xps}
ssh "$src" 'cat ~/zz-driver-build/target/debug/zz_cli' > /tmp/zz-driver-zz-dev.bin
for h in xps alienware; do
  ssh "$h" 'mkdir -p ~/.local/bin && test ! -e ~/.local/bin/zz-dev -o -n "$(grep -l zz-driver ~/.local/bin/.zz-dev-owner 2>/dev/null)"' \
    || { echo "$h already has a zz-dev that is not ours; stopping" >&2; exit 1; }
  ssh "$h" 'cat > ~/.local/bin/zz-dev && chmod +x ~/.local/bin/zz-dev && echo zz-driver > ~/.local/bin/.zz-dev-owner' < /tmp/zz-driver-zz-dev.bin
done
ssh ubuntu 'mkdir -p ~/zz-driver/bin ~/zz-driver/xdg/config/zz-dev ~/zz-driver/xdg/data/zz-dev/ssh && cat > ~/zz-driver/zz-dev && chmod +x ~/zz-driver/zz-dev' < /tmp/zz-driver-zz-dev.bin
ssh ubuntu 'bash -s' <<'REMOTE'
ln -sf ~/zz-driver/zz-dev ~/zz-driver/bin/zz
ln -sf ~/zz-driver/zz-dev ~/zz-driver/bin/zz-dev
grep -E '^host-(xps|alienware) ' ~/.config/zz/config > ~/zz-driver/xdg/config/zz-dev/config
cp ~/.local/share/zz/ssh/id_ed25519 ~/.local/share/zz/ssh/id_ed25519.pub ~/zz-driver/xdg/data/zz-dev/ssh/
chmod 600 ~/zz-driver/xdg/data/zz-dev/ssh/id_ed25519
cat > ~/zz-driver/lz <<'LZ'
#!/bin/bash
export XDG_CONFIG_HOME=$HOME/zz-driver/xdg/config XDG_DATA_HOME=$HOME/zz-driver/xdg/data PATH=$HOME/zz-driver/bin:$PATH
export ZZ_SOCKET=/tmp/zzdriver.sock
unset ZZ_PANE ZZ_SESSION TMUX TMUX_PANE
exec "$HOME/zz-driver/zz-dev" "$@"
LZ
chmod +x ~/zz-driver/lz
REMOTE
rm -f /tmp/zz-driver-zz-dev.bin
for h in xps alienware; do echo "$h: $(ssh "$h" '~/.local/bin/zz-dev --version')"; done
echo "ubuntu: $(ssh ubuntu '~/zz-driver/lz --version')"
