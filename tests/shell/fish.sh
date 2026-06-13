#!/usr/bin/env bash

tmp="$(mktemp -d)"
mkdir -p "$tmp/fish"

cat > "$tmp/fish/config.fish" <<'EOF'
cargo run --bin rw -- init fish | source
EOF

XDG_CONFIG_HOME="$tmp" fish
