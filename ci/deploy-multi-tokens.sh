#!/usr/bin/env bash

set -em

echo "Deploy USDT token"

USDT_ADDRESS=$(solana address -k /opt/keys/usdt_token_keypair.json)

spl-token -u http://localhost:8899 create-token --decimals 6 /opt/keys/usdt_token_keypair.json
spl-token -u http://localhost:8899 mint $USDT_ADDRESS 100000


echo "Deploy ETH token"

ETH_ADDRESS=$(solana address -k /opt/keys/eth_token_keypair.json)

spl-token -u http://localhost:8899 create-token --decimals 8 /opt/keys/eth_token_keypair.json
spl-token -u http://localhost:8899 mint $ETH_ADDRESS 100000