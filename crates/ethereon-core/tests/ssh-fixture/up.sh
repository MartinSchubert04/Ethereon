#!/usr/bin/env bash
# Levanta un bastion (puerto 2222 de esta PC) y un host interno ("target") que solo se
# alcanza a traves del bastion, mas uno viejo (puerto 2223) que solo acepta los
# intercambios de claves de un F5. Despues:
#   ETHEREON_TEST_KEY=<carpeta>/key cargo test -p ethereon-core --test acceptance -- --ignored
set -euo pipefail
dir="${1:-${TMPDIR:-/tmp}/ethereon-fixture}"
mkdir -p "$dir"
[ -f "$dir/key" ] || ssh-keygen -q -t ed25519 -N "" -f "$dir/key"
cp "$(dirname "$0")/Dockerfile" "$dir/"
cp "$dir/key.pub" "$dir/authorized_keys"
docker build -q -t ethereon-ssh-fixture "$dir"
docker rm -f ethereon-bastion ethereon-target ethereon-legacy >/dev/null 2>&1 || true
docker network create ethereon-net >/dev/null 2>&1 || true
docker run -d --name ethereon-bastion --network ethereon-net -p 127.0.0.1:2222:22 ethereon-ssh-fixture >/dev/null
docker run -d --name ethereon-target --network ethereon-net --network-alias target ethereon-ssh-fixture >/dev/null
# MSYS_NO_PATHCONV: en Git Bash de Windows, si no, la ruta de sshd se convierte a una de Windows.
MSYS_NO_PATHCONV=1 docker run -d --name ethereon-legacy -p 127.0.0.1:2223:22 ethereon-ssh-fixture \
  /usr/sbin/sshd -D -e -o KexAlgorithms=ecdh-sha2-nistp256,ecdh-sha2-nistp384,diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1 >/dev/null
echo "listo. ETHEREON_TEST_KEY=$dir/key"
