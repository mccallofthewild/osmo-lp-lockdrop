use std::vec;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Reply,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use osmosis_std::types::{
    cosmos::base::v1beta1::Coin,
    osmosis::gamm::{
        poolmodels::balancer::v1beta1::{MsgCreateBalancerPool, MsgCreateBalancerPoolResponse},
        v1beta1::{
            GammQuerier, MsgExitPool, MsgExitPoolResponse, Pool, PoolAsset, PoolParams,
            QueryPoolResponse, QueryTotalPoolLiquidityResponse,
        },
    },
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::{error::ContractError, msg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:osmo-lp-transfomer";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Handling contract instantiation
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // With `Response` type, it is possible to dispatch message to invoke external logic.
    // See: https://github.com/CosmWasm/cosmwasm/blob/main/SEMANTICS.md#dispatching-messages
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

/// Handling contract migration
/// To make a contract migratable, you need
/// - this entry_point implemented
/// - only contract admin can migrate, so admin has to be set at contract initiation time
/// Handling contract execution
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    match msg {
        // Find matched incoming message variant and execute them with your custom logic.
        //
        // With `Response` type, it is possible to dispatch message to invoke external logic.
        // See: https://github.com/CosmWasm/cosmwasm/blob/main/SEMANTICS.md#dispatching-messages
    }
}

/// Handling contract execution
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::EjectAndSeedLiquidity {} => execute_eject_and_seed_liquidity(deps, env, info),
        ExecuteMsg::EjectLiquidity {} => _execute_eject(deps, env, info),
        ExecuteMsg::SeedLiquidity {} => _seed_liquidity(deps, env, info),
    }
}

/// Handling contract query
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        // Find matched incoming message variant and query them your custom logic
        // and then construct your query response with the type usually defined
        // `msg.rs` alongside with the query message itself.
        //
        // use `cosmwasm_std::to_binary` to serialize query response to json binary.
    }
}

/// Handling submessage reply.
/// For more info on submessage and reply, see https://github.com/CosmWasm/cosmwasm/blob/main/SEMANTICS.md#submessages
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, _msg: Reply) -> Result<Response, ContractError> {
    // With `Response` type, it is still possible to dispatch message to invoke external logic.
    // See: https://github.com/CosmWasm/cosmwasm/blob/main/SEMANTICS.md#dispatching-messages

    todo!()
}

pub fn extract_pool_id_from_denom(denom: &str) -> Result<u64, ContractError> {
    let split: Vec<&str> = denom.split("/").collect();
    if split.len() != 3 {
        return Err(ContractError::Std(StdError::generic_err(
            "invalid pool denom",
        )));
    }
    let pool_id = split[2]
        .parse::<u64>()
        .map_err(|_e| ContractError::Std(StdError::generic_err("invalid pool denom")))?;
    Ok(pool_id)
}

pub fn execute_eject_and_seed_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // create wasm execute message for ejecting liquidity
    let eject_msg = ExecuteMsg::EjectLiquidity {};
    let eject_msg = to_binary(&eject_msg)?;
    let eject_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: eject_msg,
    });
    let seed_msg = ExecuteMsg::SeedLiquidity {};
    let seed_msg = to_binary(&seed_msg)?;
    let seed_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: seed_msg,
    });
    Ok(Response::new()
        .add_message(eject_msg)
        .add_message(seed_msg)
        .add_attribute("action", "execute_eject_and_seed_liquidity"))
}

pub fn _seed_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // only allow the contract itself to execute this
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    let seed_denom = "uosmo";
    let denom = "gamm/poolexample/5";
    let seed_amount = must_pay(&info, &seed_denom).map_err(|e| ContractError::PaymentError(e))?;
    let pool_id = extract_pool_id_from_denom(denom)?;
    let gamm_querier = GammQuerier::new(&deps.querier);
    let pool: QueryTotalPoolLiquidityResponse = gamm_querier.total_pool_liquidity(pool_id)?;
    let asset_count = pool.liquidity.len();
    let seed_amount_per_pool = seed_amount / Uint128::from(asset_count as u128);
    let seed_amount_remainder =
        seed_amount - (seed_amount_per_pool * Uint128::from(asset_count as u128));
    let msgs = pool
        .liquidity
        .iter()
        .map(|coin| -> Result<CosmosMsg, StdError> {
            let balance = deps
                .querier
                .query_balance(&env.contract.address, &coin.denom)?;
            let msg_create_balancer_pool: CosmosMsg = MsgCreateBalancerPool {
                sender: env.contract.address.to_string(),
                future_pool_governor: "24h".to_string(),
                pool_params: Some(PoolParams {
                    swap_fee: "0.003000000000000000".to_string(),
                    exit_fee: "0.000000000000000000".to_string(),
                    smooth_weight_change_params: None,
                }),
                pool_assets: vec![
                    PoolAsset {
                        token: Some(Coin {
                            denom: balance.denom,
                            amount: balance.amount.to_string(),
                        }),
                        weight: "536870912000000".to_string(),
                    },
                    PoolAsset {
                        token: Some(Coin {
                            denom: seed_denom.to_string(),
                            amount: seed_amount_per_pool.to_string(),
                        }),
                        weight: "536870912000000".to_string(),
                    },
                ],
            }
            .into();
            Ok(msg_create_balancer_pool)
        })
        .map(|msg| msg)
        .collect::<Result<Vec<_>, _>>()?;
    let bank_transfer_remainder: CosmosMsg<Empty> = CosmosMsg::Bank(BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(seed_amount_remainder.u128(), seed_denom),
    });
    Ok(Response::new()
        .add_attribute("action", "execute_eject_and_seed_liquidity")
        .add_messages(msgs)
        .add_message(bank_transfer_remainder))
}

pub fn _execute_eject(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // only allow the contract itself to execute this
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    let denom = "gamm/poolexample/5";
    let balance = deps.querier.query_balance(&env.contract.address, denom)?;
    let msg_exit_pool: CosmosMsg = MsgExitPool {
        sender: env.contract.address.to_string(),
        pool_id: 1,
        share_in_amount: balance.amount.to_string(),
        token_out_mins: vec![],
    }
    .into();
    Ok(Response::new().add_message(msg_exit_pool))
}
