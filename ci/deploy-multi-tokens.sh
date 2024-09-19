#!/usr/bin/env bash

set -em

SOLANA_URL="${SOLANA_URL:-default}" 

echo "Deploy USDT token"

USDT_ADDRESS=$(solana address -k /opt/keys/usdt_token_keypair.json)

spl-token -u $SOLANA_URL create-token --decimals 6 /opt/keys/usdt_token_keypair.json
spl-token -u $SOLANA_URL create-account $USDT_ADDRESS
spl-token -u $SOLANA_URL mint $USDT_ADDRESS 100000000000


echo "Deploy ETH token"

ETH_ADDRESS=$(solana address -k /opt/keys/eth_token_keypair.json)

spl-token -u $SOLANA_URL create-token --decimals 8 /opt/keys/eth_token_keypair.json
spl-token -u $SOLANA_URL create-account $ETH_ADDRESS
spl-token -u $SOLANA_URL mint $ETH_ADDRESS 100000000000
