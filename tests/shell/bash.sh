#!/usr/bin/env bash

tmp="$(mktemp -d)"

cat > "$tmp/bashrc" <<'EOF'
eval "$(cargo run --bin rw -- init bash)"
EOF

bash --rcfile "$tmp/bashrc" -i
