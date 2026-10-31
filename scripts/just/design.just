import "settings.just"

set working-directory := "../.."

[doc("Write the CSS tokens of every built-in theme, with zz's roles, into design/tokens.")]
tokens:
    cargo run -q -p zz-ui --example theme_css design/tokens

[doc("Write penna's own CSS tokens, the kit's roles only, into libs/penna/web/tokens.")]
penna-tokens:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/penna-locks.sh
    cargo run -q --manifest-path libs/penna/Cargo.toml --config "patch.\"https://github.com/autistic-computer/ala\".ala.path=\"$PWD/libs/ala/crates/ala\"" --example theme_css

[doc("Serve design/ on 127.0.0.1:8090 without caching, with penna's kit page at /penna/.")]
serve: tokens penna-tokens
    @scripts/storybook-serve.py design 8090 penna=libs/penna/web

[doc("Build the HTML thin client and serve it through the gateway on 127.0.0.1:8092; --socket picks the daemon.")]
client *args:
    scripts/build-dom.sh
    ZZ_DEV_BUILD=1 cargo build -q -p zz-web
    target/debug/zz-web --assets clients/dom/dist --bind 127.0.0.1:8092 {{ args }}

[doc("Serve the HTML client (8092) and the ala web client (8093) on one daemon; a scratch daemon on /tmp/zzdom.sock by default.")]
compare socket="/tmp/zzdom.sock":
    scripts/dom-dev.sh {{ socket }}

[doc("Diff the HTML client against the ala web client that `just design compare` serves.")]
parity *args:
    node scripts/dom-parity.mjs {{ args }}
