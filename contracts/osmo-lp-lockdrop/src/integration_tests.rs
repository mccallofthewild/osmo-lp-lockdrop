use cosmwasm_std::Coin;
use cw_utils::Duration;
use osmosis_testing::{Account, Bank, Gamm, Module, OsmosisTestApp, Wasm};

use crate::msg::{ExecuteMsg, InstantiateMsg};

#[test]
fn setup() {
    let app = OsmosisTestApp::default();
    let alice = app
        .init_account(&[
            Coin::new(1_000_000_000_000, "uatom"),
            Coin::new(1_000_000_000_000, "uosmo"),
        ])
        .unwrap();

    // create Gamm Module Wrapper
    let gamm = Gamm::new(&app);

    // create balancer pool with basic configuration
    let pool_liquidity = vec![Coin::new(1_000, "uatom"), Coin::new(1_000, "uosmo")];
    let pool_id = gamm
        .create_basic_pool(&pool_liquidity, &alice)
        .unwrap()
        .data
        .pool_id;

    // query pool and assert if the pool is created successfully
    let pool = gamm.query_pool(pool_id).unwrap();
    assert_eq!(
        pool_liquidity
            .into_iter()
            .map(|c| c.into())
            .collect::<Vec<osmosis_std::types::cosmos::base::v1beta1::Coin>>(),
        pool.pool_assets
            .into_iter()
            .map(|a| a.token.unwrap())
            .collect::<Vec<osmosis_std::types::cosmos::base::v1beta1::Coin>>(),
    );
    let wasm = Wasm::new(&app);
    let wasm_byte_code = std::fs::read("./artifacts/osmo_lp_lockdrop.wasm").unwrap();
    let code_id = wasm
        .store_code(&wasm_byte_code, None, &alice)
        .unwrap()
        .data
        .code_id;
    let lockdrop_byte_code = std::fs::read("./artifacts/lockdrop_rewards.wasm").unwrap();
    let lockdrop_code_id = wasm
        .store_code(&wasm_byte_code, None, &alice)
        .unwrap()
        .data
        .code_id;
    // instantiate contract with initial admin and make admin list mutable
    let init_admins = vec![alice.address()];
    let pool_denom = format!("gamm/pool/{}", pool_id);
    let contract_addr = wasm
        .instantiate(
            code_id,
            &InstantiateMsg {
                owner: Some(alice.address()),
                manager: Some(alice.address()),
                denom: pool_denom.clone(),
                unstaking_duration: Some(Duration::Time(60)),
                reward_contract_code_id: lockdrop_code_id,
            },
            None,   // contract admin used for migration, not the same as cw1_whitelist admin
            None,   // contract label
            &[],    // funds
            &alice, // signer
        )
        .unwrap()
        .data
        .address;

    // query contract state to check if contract instantiation works properly
    // stake
    wasm.execute(
        &contract_addr,
        &ExecuteMsg::Stake {},
        &[Coin::new(1000, pool_denom.clone())],
        &alice,
    )
    .unwrap();

    let bank = Bank::new(&app);
    let balances = bank.query_all_balances(&alice.address(), None).unwrap();
    let useed_balance = balances
        .balances
        .iter()
        .find(|c| c.denom == "useed")
        .unwrap()
        .amount
        .clone();
    assert_eq!(useed_balance, "1000");
}
