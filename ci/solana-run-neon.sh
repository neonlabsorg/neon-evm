#!/usr/bin/env bash

set -em

NEON_BIN=/opt

EVM_LOADER_AUTHORITY_KEYPAIR=${NEON_BIN}/evm_loader-keypair.json
EVM_LOADER_PROGRAM_ID_KEYPAIR=${NEON_BIN}/evm_loader-keypair.json
EVM_LOADER=$(solana address -k ${EVM_LOADER_PROGRAM_ID_KEYPAIR})
EVM_LOADER_PATH=${NEON_BIN}/evm_loader.so

METAPLEX=metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s
METAPLEX_PATH=${NEON_BIN}/metaplex.so

PYTH_SOL_ID=7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE
PYTH_NEON_ID=F2VfCymdNQiCa8Vyg5E7BwEv9UPwfm8cVN6eqQLqXiGo
PYTH_ETH_ID=42amVS4KgzR9rA28tkVYqVXjq9Qa8dcZQMbH5EYFX6XC
PUTH_USDC_ID=Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX
PUTH_USDT_ID=HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM
WSOL_ID=6dM4TqWyWJsbx7obrdLcviBkTafD5E8av61zfU6jq57X
PYTH_SOL_PATH=${NEON_BIN}/pyth_sol.json
PYTH_NEON_PATH=${NEON_BIN}/pyth_neon.json
PYTH_ETH_PATH=${NEON_BIN}/pyth_eth.json
PUTH_USDC_PATH=${NEON_BIN}/pyth_usdc.json
PUTH_USDT_PATH=${NEON_BIN}/pyth_usdt.json
WSOL_PATH=${NEON_BIN}/wsol.json

if [ -n "$DEVNET_SOLANA_URL" ]; then
    url=$DEVNET_SOLANA_URL
    echo "DEVNET_SOLANA_URL variable found."
else
    url=mainnet-beta
    echo "DEVNET_SOLANA_URL variable not found. Pyth accounts will be fetched from mainnet."
fi

solana account 7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE --output-file pyth_sol.json --output json-compact --url ${url}
solana account F2VfCymdNQiCa8Vyg5E7BwEv9UPwfm8cVN6eqQLqXiGo --output-file pyth_neon.json --output json-compact --url ${url}
solana account 42amVS4KgzR9rA28tkVYqVXjq9Qa8dcZQMbH5EYFX6XC --output-file pyth_eth.json --output json-compact --url ${url}
solana account Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX --output-file pyth_usdc.json --output json-compact --url ${url}
solana account HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM --output-file pyth_usdt.json --output json-compact --url ${url}
solana account 6dM4TqWyWJsbx7obrdLcviBkTafD5E8av61zfU6jq57X --output-file wsol.json --output json-compact --url ${url}

VALIDATOR_ARGS=(
  --reset
  --warp-slot 1
  --log-messages-bytes-limit 50000
  --ticks-per-slot 16
  --upgradeable-program ${EVM_LOADER} ${EVM_LOADER_PATH} ${EVM_LOADER_AUTHORITY_KEYPAIR}
  --bpf-program ${METAPLEX} ${METAPLEX_PATH}
  --account ${PYTH_SOL_ID} ${PYTH_SOL_PATH}
  --account ${PYTH_NEON_ID} ${PYTH_NEON_PATH}
  --account ${PYTH_ETH_ID} ${PYTH_ETH_PATH}
  --account ${PUTH_USDC_ID} ${PUTH_USDC_PATH}
  --account ${PUTH_USDT_ID} ${PUTH_USDT_PATH}
  --account ${WSOL_ID} ${WSOL_PATH}
  --limit-ledger-size 400000000
)

LIST_OF_TEST_PROGRAMS=("test_invoke_program" "counter" "cross_program_invocation" "transfer_sol" "transfer_tokens")

for program in "${LIST_OF_TEST_PROGRAMS[@]}"; do
  keypair="${NEON_BIN}/deploy/${program}/${program}-keypair.json"
  address=$(solana address -k $keypair)
  VALIDATOR_ARGS+=(--bpf-program $address ${NEON_BIN}/deploy/$program/$program.so)
done

if [[ -n $GEYSER_PLUGIN_CONFIG ]]; then
  echo "Using geyser plugin with config: $GEYSER_PLUGIN_CONFIG"
  VALIDATOR_ARGS+=(--geyser-plugin-config $GEYSER_PLUGIN_CONFIG)
fi

export RUST_LOG=solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=debug,solana_bpf_loader=debug,solana_rbpf=debug
solana-test-validator "${VALIDATOR_ARGS[@]}" > /dev/null &
./wait-for-solana.sh ${SOLANA_WAIT_TIMEOUT:-60}
./deploy-multi-tokens.sh

neon-cli --url http://localhost:8899 --evm_loader $EVM_LOADER \
  --commitment confirmed \
  --keypair ${EVM_LOADER_AUTHORITY_KEYPAIR} \
  --solana_key_for_config BMp6gEnveANdvSvspESJUrNczuHz1GF5UQKjVLCkAZih \
  --loglevel trace init-environment --send-trx --keys-dir /opt/keys

tail +1f test-ledger/validator.log
