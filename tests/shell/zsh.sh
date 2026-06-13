#!/usr/bin/env bash

tmp="$(mktemp -d)"

cat > "$tmp/.zshrc" <<'EOF'
eval "$(cargo run --bin rw -- init zsh)"
EOF

ZDOTDIR="$tmp" zsh -i
