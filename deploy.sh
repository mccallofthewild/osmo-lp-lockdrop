# beaker wasm build

# beaker wasm store-code lockdrop-rewards \
#   --network testnet \
#   --signer-account test1


beaker wasm deploy osmo-lp-lockdrop \
  --network testnet \
  --signer-account test1 \
  --no-wasm-opt \
  --raw '{ "reward_contract_code_id": 2416, "owner": "osmo1cyyzpxplxdzkeea7kwsydadg87357qnahakaks", "manager": "osmo1cyyzpxplxdzkeea7kwsydadg87357qnahakaks", "denom": "gamm/pool/560", "unstaking_duration": { "time": 31536000 } }' \
  --admin signer 
# time = time in seconds
# Testnet balances of test1
# > osmosisd q bank balances osmo1cyyzpxplxdzkeea7kwsydadg87357qnahakaks --node https://rpc-test.osmosis.zone:443
# balances:
# - amount: "100000000"
#   denom: gamm/pool/1
# - amount: "200000000"
#   denom: gamm/pool/560
# - amount: "8827907463"
#   denom: ibc/E6931F78057F7CC5DA0FD6CEF82FF39373A6E0452BF1FD76910B93292CF356C1
# - amount: "82770186633"
#   denom: uosmo
# pagination:
#   next_key: null
#   total: "0"