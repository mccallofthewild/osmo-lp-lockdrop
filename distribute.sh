# creates reward distributors for _all_ tokens in contract 
beaker wasm execute osmo-lp-lockdrop --signer-account test4 --raw '{ "distribute_all_tokens": {} }' --network testnet 

beaker wasm query osmo-lp-lockdrop --network testnet --raw '{ "all_reward_contracts": {}  }'