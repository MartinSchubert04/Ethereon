#!/usr/bin/env bash
docker rm -f ethereon-bastion ethereon-target >/dev/null 2>&1 || true
docker network rm ethereon-net >/dev/null 2>&1 || true
