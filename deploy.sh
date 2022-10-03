# beaker wasm build

# beaker wasm store-code lockdrop-rewards \
#   --network testnet \
#   --signer-account test4


beaker wasm deploy osmo-lp-lockdrop \
  --network testnet \
  --signer-account test4 \
  --no-wasm-opt \
  --raw '{ "reward_contract_code_id": 2416, "owner": "osmo14hcxlnwlqtq75ttaxf674vk6mafspg8xwgnn53", "manager": "osmo14hcxlnwlqtq75ttaxf674vk6mafspg8xwgnn53", "denom": "gamm/pool/1", "unstaking_duration": { "time": 31536000 } }' \
  --admin signer \
  --funds "200000000uosmo"


# time = time in seconds
# Testnet balances of test4
# > osmosisd q bank balances osmo14hcxlnwlqtq75ttaxf674vk6mafspg8xwgnn53 --node https://rpc-test.osmosis.zone:443
# balances:
# - amount: "100000000"
#   denom: gamm/pool/1
# - amount: "200000000"
#   denom: gamm/pool/1
# - amount: "8827907463"
#   denom: ibc/E6931F78057F7CC5DA0FD6CEF82FF39373A6E0452BF1FD76910B93292CF356C1
# - amount: "82770186633"
#   denom: uosmo
# pagination:
#   next_key: null
#   total: "0"


# osmosisd q bank balances osmo14hcxlnwlqtq75ttaxf674vk6mafspg8xwgnn53 --node https://rpc-test.osmosis.zone:443

# balances:
# - amount: "762444095584240531064"
#   denom: gamm/pool/1
# - amount: "3046358"
#   denom: ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2
# - amount: "8789997600"
#   denom: uosmo
# pagination:
#   next_key: null
#   total: "0"