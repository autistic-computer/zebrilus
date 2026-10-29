import "settings.just"

set working-directory := "../.."

# Write the CSS tokens of every built-in theme into design/kit/tokens.
tokens:
    cargo run -q -p zz-ui --example theme_css design/kit/tokens

# Serve design/ on 127.0.0.1:8090 without caching.
serve: tokens
    @scripts/storybook-serve.py design 8090

# Build the HTML thin client and serve it through the gateway on 127.0.0.1:8092; --socket picks the daemon.
client *args:
    scripts/build-dom.sh
    ZZ_DEV_BUILD=1 cargo build -q -p zz-web
    target/debug/zz-web --assets clients/dom/dist --bind 127.0.0.1:8092 {{ args }}

# Serve the HTML client (8092) and the ala web client (8093) on one daemon; a scratch daemon on /tmp/zzdom.sock by default.
compare socket="/tmp/zzdom.sock":
    scripts/dom-dev.sh {{ socket }}

# Diff the HTML client against the ala web client that `just design compare` serves.
parity *args:
    node scripts/dom-parity.mjs {{ args }}
