#!/usr/bin/env bash
# Levanta un bastion (puerto 2222 de esta PC) y un host interno ("target") que solo se
# alcanza a traves del bastion. Despues:
#   ETHEREON_TEST_KEY=<carpeta>/key cargo test -p ethereon-core --test acceptance -- --ignored
set -euo pipefail
dir="${1:-${TMPDIR:-/tmp}/ethereon-fixture}"
mkdir -p "$dir"
[ -f "$dir/key" ] || ssh-keygen -q -t ed25519 -N "" -f "$dir/key"
cp "$(dirname "$0")/Dockerfile" "$dir/"
cp "$dir/key.pub" "$dir/authorized_keys"
docker build -q -t ethereon-ssh-fixture "$dir"
docker rm -f ethereon-bastion ethereon-target >/dev/null 2>&1 || true
docker network create ethereon-net >/dev/null 2>&1 || true
docker run -d --name ethereon-bastion --network ethereon-net -p 127.0.0.1:2222:22 ethereon-ssh-fixture >/dev/null
docker run -d --name ethereon-target --network ethereon-net --network-alias target ethereon-ssh-fixture >/dev/null
echo "listo. ETHEREON_TEST_KEY=$dir/key"
