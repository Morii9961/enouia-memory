#!/usr/bin/env sh
# Builds the Linux remote-service bundle from a clean checkout on Linux (for
# example WSL2): release binaries, the deploy templates and their documents,
# packed as .local/dist/enouia-memory-remote-<commit>-<arch>.tar.gz with a
# SHA256SUMS file. It builds and packs only; it installs and deploys nothing.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
if [ -n "$(git status --porcelain)" ]; then
    echo "package_linux: the checkout has uncommitted changes" >&2
    exit 1
fi
commit=$(git rev-parse --short=12 HEAD)
arch=$(uname -m)
name="enouia-memory-remote-$commit-$arch"
export CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"
cargo build --release --locked \
    -p enouia-memory-host --bin enouia-memory-remote --bin enouia-memory-owner-bootstrap \
    -p enouia-memory-cli --bin enouia-memory-owner-backup
target="${CARGO_TARGET_DIR:-$root/target}/release"
dist="$root/.local/dist"
stage="$dist/$name"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/deploy"
for bin in enouia-memory-remote enouia-memory-owner-bootstrap enouia-memory-owner-backup; do
    install -m 0755 "$target/$bin" "$stage/bin/$bin"
done
cp docs/deploy/linux/* "$stage/deploy/"
cp docs/deploy/REMOTE_MCP_LINUX.md docs/deploy/CONNECTOR_READ_GRANTS.md \
    docs/deploy/CHATGPT_CONNECTOR_USAGE.md docs/deploy/SEGMENTED_MORII_BACKUP.md "$stage/deploy/"
printf '%s\n' "$commit" > "$stage/COMMIT"
(cd "$stage" && find . -type f ! -name SHA256SUMS | sort | xargs sha256sum > SHA256SUMS)
tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"
(cd "$dist" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "$dist/$name.tar.gz"
