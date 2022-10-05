#!/bin/bash
set -euo pipefail

echo "Neon EVM revision=${BUILDKITE_COMMIT}"

#set ${SOLANA_PROVIDER:=solanalabs}
#set ${SOLANA_REVISION:=v1.11.10}
set ${SOLANA_REVISION:=v1.15.0}

#export SOLANA_IMAGE=${SOLANA_PROVIDER}/solana:${SOLANA_REVISION}
export SOLANA_IMAGE=neonlabsorg/neon-validator:53511e355c1e8b54c1040a651a879366209550b1
echo "SOLANA_IMAGE=${SOLANA_IMAGE}"
docker pull ${SOLANA_IMAGE}

docker build --build-arg REVISION=${BUILDKITE_COMMIT} --build-arg SOLANA_IMAGE=${SOLANA_IMAGE} --build-arg SOLANA_REVISION=${SOLANA_REVISION} -t neonlabsorg/evm_loader:${BUILDKITE_COMMIT} .
