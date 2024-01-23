#!/usr/bin/env bash

set -e

echo "Build contracts"
(cd contracts && forge build)

echo "Generate contract bindings for javascript"
(cd web/packages/contract-types && pnpm typechain && pnpm build)

echo "Generate contract bindings for go"
(cd relayer && mage build)

echo "Generate contract and substrate bindings for rust"
(cd smoketest && ./make-bindings.sh)
