#!/bin/bash
set -euo pipefail

#export BUILDKITE_COMMIT=9ecbc75f94801009e300f266cf97f3ac475190d6
echo "Neon EVM revision=${BUILDKITE_COMMIT}"

#set ${SOLANA_PROVIDER:=solanalabs}
#set ${SOLANA_REVISION:=v1.11.10}
set ${SOLANA_REVISION:=v1.14.5}


#export SOLANA_IMAGE=${SOLANA_PROVIDER}/solana:${SOLANA_REVISION}
export SOLANA_IMAGE=neonlabsorg/neon-validator:4eeef4d31564a8a3914b84a31e40f72f4ff81e31
#export SOLANA_IMAGE=neonlabs/solana:v1.15.0
echo "SOLANA_IMAGE=${SOLANA_IMAGE}"
docker pull ${SOLANA_IMAGE}

docker build --build-arg REVISION=${BUILDKITE_COMMIT} --build-arg SOLANA_IMAGE=${SOLANA_IMAGE} --build-arg SOLANA_REVISION=${SOLANA_REVISION} -t neonlabsorg/evm_loader:${BUILDKITE_COMMIT} .
