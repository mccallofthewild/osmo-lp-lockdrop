use std::ops::Mul;
use std::str::FromStr;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    coins, to_binary, Addr, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo,
    Order, Reply, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::Denom;

use crate::hooks::{stake_hook_msgs, unstake_hook_msgs};
use crate::msg::{
    AllRewardContractsResponse, ExecuteMsg, GetHooksResponse, InstantiateMsg, ListStakersResponse,
    QueryMsg, StakedBalanceAtHeightResponse, StakedValueResponse, StakerBalanceResponse,
    TotalStakedAtHeightResponse, TotalValueResponse,
};
use crate::state::{
    Config, BALANCE, CLAIMS, CONFIG, HOOKS, MAX_CLAIMS, REWARD_CONTRACTS_BY_DENOM, STAKED_BALANCES,
    STAKED_TOTAL,
};
use crate::ContractError;
use cw2::set_contract_version;
pub use cw20_base::allowances::{
    execute_burn_from, execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from, query_allowance,
};
pub use cw20_base::contract::{
    execute_burn, execute_mint, execute_send, execute_transfer, execute_update_marketing,
    execute_upload_logo, query_balance, query_download_logo, query_marketing_info, query_minter,
    query_token_info,
};
pub use cw20_base::enumerable::{query_all_accounts, query_all_allowances};
use cw_controllers::ClaimsResponse;
use cw_utils::{must_pay, parse_reply_instantiate_data, Duration};
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

pub(crate) const CONTRACT_NAME: &str = "crates.io:native-stake";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn validate_duration(duration: Option<Duration>) -> Result<(), ContractError> {
    if let Some(unstaking_duration) = duration {
        match unstaking_duration {
            Duration::Height(height) => {
                if height == 0 {
                    return Err(ContractError::InvalidUnstakingDuration {});
                }
            }
            Duration::Time(time) => {
                if time == 0 {
                    return Err(ContractError::InvalidUnstakingDuration {});
                }
            }
        }
    }
    Ok(())
}

// handle reply
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    let res = parse_reply_instantiate_data(msg)
        .map_err(|e| ContractError::Std(StdError::generic_err("failed to instantiate")))?;
    let contract_addr = deps.api.addr_validate(&res.contract_address)?;
    // query info about lockdrop rewards contract
    let info: lockdrop_rewards::msg::InfoResponse = deps
        .querier
        .query_wasm_smart(&contract_addr, &lockdrop_rewards::msg::QueryMsg::Info {})?;

    let denom = match info.config.reward_token {
        Denom::Native(denom) => denom,
        Denom::Cw20(_) => return Err(ContractError::InvalidDenom {}),
    };

    REWARD_CONTRACTS_BY_DENOM.save(deps.storage, &denom, &contract_addr)?;
    let fund_rewards_contract_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: to_binary(&ExecuteMsg::FundRewardsContract {
            denom: denom.clone(),
        })?,
    });
    Ok(Response::new().add_message(fund_rewards_contract_msg))
}

pub fn execute_fund_rewards_contract(
    deps: DepsMut,
    env: Env,
    denom: String,
) -> Result<Response, ContractError> {
    let reward_contract_addr = REWARD_CONTRACTS_BY_DENOM.load(deps.storage, &denom)?;
    let info: lockdrop_rewards::msg::InfoResponse = deps.querier.query_wasm_smart(
        &reward_contract_addr,
        &lockdrop_rewards::msg::QueryMsg::Info {},
    )?;

    let denom = match info.config.reward_token {
        Denom::Native(denom) => denom,
        Denom::Cw20(_) => return Err(ContractError::InvalidDenom {}),
    };
    let balance = deps.querier.query_balance(&env.contract.address, denom)?;
    let fund_lockdrop_rewards_msg: CosmosMsg<Empty> = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_contract_addr.to_string(),
        funds: vec![balance],
        msg: to_binary(&lockdrop_rewards::msg::ExecuteMsg::Fund {})?,
    });

    Ok(Response::new().add_message(fund_lockdrop_rewards_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, ContractError> {
    let owner = match msg.owner {
        Some(owner) => Some(deps.api.addr_validate(owner.as_str())?),
        None => None,
    };

    let manager = match msg.manager {
        Some(manager) => Some(deps.api.addr_validate(manager.as_str())?),
        None => None,
    };

    validate_duration(msg.unstaking_duration)?;
    let config = Config {
        owner,
        manager,
        denom: msg.denom,
        unstaking_duration: msg.unstaking_duration,
        reward_contract_code_id: msg.reward_contract_code_id,
    };
    CONFIG.save(deps.storage, &config)?;

    // Initialize state to zero. We do this instead of using
    // `unwrap_or_default` where this is used as it protects us
    // against a scenerio where state is cleared by a bad actor and
    // `unwrap_or_default` carries on.
    STAKED_TOTAL.save(deps.storage, &Uint128::zero(), env.block.height)?;
    BALANCE.save(deps.storage, &Uint128::zero())?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::DistributeAllTokens {} => execute_distribute_all_tokens(deps, env, info),
        ExecuteMsg::DistributeToken { denom } => execute_distribute_token(deps, env, info, denom),
        ExecuteMsg::FundRewardsContract { denom } => {
            execute_fund_rewards_contract(deps, env, denom)
        }
        ExecuteMsg::EjectAndSeedLiquidity {
            seed_denom,
            gamm_denom,
        } => execute_eject_and_seed_liquidity(deps, env, info, seed_denom, gamm_denom),
        ExecuteMsg::EjectLiquidity { gamm_denom } => _execute_eject(deps, env, info, gamm_denom),
        ExecuteMsg::SeedLiquidity {
            seed_denom,
            gamm_denom,
        } => _seed_liquidity(deps, env, info, seed_denom, gamm_denom),
        ExecuteMsg::Fund {} => execute_fund(deps, env, info),
        ExecuteMsg::Stake {} => execute_stake(deps, env, info),
        ExecuteMsg::Unstake { amount } => execute_unstake(deps, env, info, amount),
        ExecuteMsg::Claim {} => execute_claim(deps, env, info),
        ExecuteMsg::UpdateConfig {
            owner,
            manager,
            duration,
        } => execute_update_config(info, deps, owner, manager, duration),
        ExecuteMsg::AddHook { addr } => execute_add_hook(deps, env, info, addr),
        ExecuteMsg::RemoveHook { addr } => execute_remove_hook(deps, env, info, addr),
    }
}

pub fn execute_update_config(
    info: MessageInfo,
    deps: DepsMut,
    new_owner: Option<String>,
    new_manager: Option<String>,
    duration: Option<Duration>,
) -> Result<Response, ContractError> {
    let new_owner = new_owner
        .map(|new_owner| deps.api.addr_validate(&new_owner))
        .transpose()?;
    let new_manager = new_manager
        .map(|new_manager| deps.api.addr_validate(&new_manager))
        .transpose()?;
    let mut config: Config = CONFIG.load(deps.storage)?;
    if Some(info.sender.clone()) != config.owner && Some(info.sender.clone()) != config.manager {
        return Err(ContractError::Unauthorized {});
    };
    if Some(info.sender) != config.owner && new_owner != config.owner {
        return Err(ContractError::OnlyOwnerCanChangeOwner {});
    };

    validate_duration(duration)?;

    config.owner = new_owner;
    config.manager = new_manager;

    config.unstaking_duration = duration;

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute(
            "owner",
            config
                .owner
                .map(|a| a.to_string())
                .unwrap_or_else(|| "None".to_string()),
        )
        .add_attribute(
            "manager",
            config
                .manager
                .map(|a| a.to_string())
                .unwrap_or_else(|| "None".to_string()),
        ))
}

pub fn execute_stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, &config.denom)?;
    let balance = BALANCE.load(deps.storage)?;
    let staked_total = STAKED_TOTAL.load(deps.storage)?;
    let amount_to_stake = if staked_total == Uint128::zero() || balance == Uint128::zero() {
        amount
    } else {
        staked_total
            .checked_mul(amount)
            .map_err(StdError::overflow)?
            .checked_div(balance)
            .map_err(StdError::divide_by_zero)?
    };
    STAKED_BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance| -> StdResult<Uint128> {
            Ok(balance.unwrap_or_default().checked_add(amount_to_stake)?)
        },
    )?;
    STAKED_TOTAL.update(
        deps.storage,
        env.block.height,
        |total| -> StdResult<Uint128> {
            Ok(total.unwrap_or_default().checked_add(amount_to_stake)?)
        },
    )?;
    BALANCE.save(
        deps.storage,
        &balance.checked_add(amount).map_err(StdError::overflow)?,
    )?;
    let hook_msgs = stake_hook_msgs(deps.storage, info.sender.clone(), amount_to_stake)?;
    Ok(Response::new()
        .add_submessages(hook_msgs)
        .add_attribute("action", "stake")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount))
}

pub fn execute_unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let balance = BALANCE.load(deps.storage)?;
    let staked_total = STAKED_TOTAL.load(deps.storage)?;
    let amount_to_claim = amount
        .checked_mul(balance)
        .map_err(StdError::overflow)?
        .checked_div(staked_total)
        .map_err(|_e| ContractError::InvalidUnstakeAmount {})?;
    STAKED_BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance| -> Result<Uint128, ContractError> {
            balance
                .unwrap_or_default()
                .checked_sub(amount)
                .map_err(|_e| ContractError::InvalidUnstakeAmount {})
        },
    )?;
    STAKED_TOTAL.update(
        deps.storage,
        env.block.height,
        |total| -> Result<Uint128, ContractError> {
            total
                .unwrap_or_default()
                .checked_sub(amount)
                .map_err(|_e| ContractError::InvalidUnstakeAmount {})
        },
    )?;
    BALANCE.update(deps.storage, |bal| -> Result<Uint128, ContractError> {
        bal.checked_sub(amount_to_claim)
            .map_err(|_e| ContractError::InvalidUnstakeAmount {})
    })?;
    let hook_msgs = unstake_hook_msgs(deps.storage, info.sender.clone(), amount)?;
    match config.unstaking_duration {
        None => {
            let msg = CosmosMsg::Bank(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: coins(amount_to_claim.u128(), config.denom),
            });
            Ok(Response::new()
                .add_message(msg)
                .add_submessages(hook_msgs)
                .add_attribute("action", "unstake")
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", "None"))
        }
        Some(duration) => {
            let outstanding_claims = CLAIMS.query_claims(deps.as_ref(), &info.sender)?.claims;
            if outstanding_claims.len() >= MAX_CLAIMS as usize {
                return Err(ContractError::TooManyClaims {});
            }

            CLAIMS.create_claim(
                deps.storage,
                &info.sender,
                amount_to_claim,
                duration.after(&env.block),
            )?;
            Ok(Response::new()
                .add_attribute("action", "unstake")
                .add_submessages(hook_msgs)
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", format!("{}", duration)))
        }
    }
}

pub fn execute_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let release = CLAIMS.claim_tokens(deps.storage, &info.sender, &env.block, None)?;
    if release.is_zero() {
        return Err(ContractError::NothingToClaim {});
    }

    let config = CONFIG.load(deps.storage)?;
    let msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(release.u128(), config.denom),
    });

    Ok(Response::new()
        .add_message(msg)
        .add_attribute("action", "claim")
        .add_attribute("from", info.sender)
        .add_attribute("amount", release))
}

pub fn execute_fund(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, &config.denom)?;
    BALANCE.update(deps.storage, |balance| -> StdResult<_> {
        balance.checked_add(amount).map_err(StdError::overflow)
    })?;
    Ok(Response::new()
        .add_attribute("action", "fund")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount))
}

pub fn execute_add_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    addr: String,
) -> Result<Response, ContractError> {
    let addr = deps.api.addr_validate(&addr)?;
    let config: Config = CONFIG.load(deps.storage)?;
    if config.owner != Some(info.sender.clone()) && config.manager != Some(info.sender) {
        return Err(ContractError::Unauthorized {});
    };
    HOOKS.add_hook(deps.storage, addr.clone())?;
    Ok(Response::new()
        .add_attribute("action", "add_hook")
        .add_attribute("hook", addr))
}

pub fn execute_remove_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    addr: String,
) -> Result<Response, ContractError> {
    let addr = deps.api.addr_validate(&addr)?;
    let config: Config = CONFIG.load(deps.storage)?;
    if config.owner != Some(info.sender.clone()) && config.manager != Some(info.sender) {
        return Err(ContractError::Unauthorized {});
    };
    HOOKS.remove_hook(deps.storage, addr.clone())?;
    Ok(Response::new()
        .add_attribute("action", "remove_hook")
        .add_attribute("hook", addr))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::AllRewardContracts {} => to_binary(&query_all_reward_contracts(deps, env)?),
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::StakedBalanceAtHeight { address, height } => {
            to_binary(&query_staked_balance_at_height(deps, env, address, height)?)
        }
        QueryMsg::TotalStakedAtHeight { height } => {
            to_binary(&query_total_staked_at_height(deps, env, height)?)
        }
        QueryMsg::StakedValue { address } => to_binary(&query_staked_value(deps, env, address)?),
        QueryMsg::TotalValue {} => to_binary(&query_total_value(deps, env)?),
        QueryMsg::Claims { address } => to_binary(&query_claims(deps, address)?),
        QueryMsg::GetHooks {} => to_binary(&query_hooks(deps)?),
        QueryMsg::ListStakers { start_after, limit } => {
            query_list_stakers(deps, start_after, limit)
        }
    }
}

pub fn query_all_reward_contracts(deps: Deps, _env: Env) -> StdResult<AllRewardContractsResponse> {
    let all_reward_contracts_by_denom: Vec<String> = REWARD_CONTRACTS_BY_DENOM
        .range(deps.storage, None, None, Order::Descending)
        .collect::<StdResult<Vec<(String, Addr)>>>()?
        .iter()
        .map(|(_, v)| v.to_string())
        .collect();
    Ok(AllRewardContractsResponse {
        reward_contracts: all_reward_contracts_by_denom,
    })
}

pub fn query_staked_balance_at_height(
    deps: Deps,
    _env: Env,
    address: String,
    height: Option<u64>,
) -> StdResult<StakedBalanceAtHeightResponse> {
    let address = deps.api.addr_validate(&address)?;
    let height = height.unwrap_or(_env.block.height);
    let balance = STAKED_BALANCES
        .may_load_at_height(deps.storage, &address, height)?
        .unwrap_or_default();
    Ok(StakedBalanceAtHeightResponse { balance, height })
}

pub fn query_total_staked_at_height(
    deps: Deps,
    _env: Env,
    height: Option<u64>,
) -> StdResult<TotalStakedAtHeightResponse> {
    let height = height.unwrap_or(_env.block.height);
    let total = STAKED_TOTAL
        .may_load_at_height(deps.storage, height)?
        .unwrap_or_default();
    Ok(TotalStakedAtHeightResponse { total, height })
}

pub fn query_staked_value(
    deps: Deps,
    _env: Env,
    address: String,
) -> StdResult<StakedValueResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCE.load(deps.storage).unwrap_or_default();
    let staked = STAKED_BALANCES
        .load(deps.storage, &address)
        .unwrap_or_default();
    let total = STAKED_TOTAL.load(deps.storage)?;
    if balance == Uint128::zero() || staked == Uint128::zero() || total == Uint128::zero() {
        Ok(StakedValueResponse {
            value: Uint128::zero(),
        })
    } else {
        let value = staked
            .checked_mul(balance)
            .map_err(StdError::overflow)?
            .checked_div(total)
            .map_err(StdError::divide_by_zero)?;
        Ok(StakedValueResponse { value })
    }
}

pub fn query_total_value(deps: Deps, _env: Env) -> StdResult<TotalValueResponse> {
    let balance = BALANCE.load(deps.storage)?;
    Ok(TotalValueResponse { total: balance })
}

pub fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

pub fn query_claims(deps: Deps, address: String) -> StdResult<ClaimsResponse> {
    CLAIMS.query_claims(deps, &deps.api.addr_validate(&address)?)
}

pub fn query_hooks(deps: Deps) -> StdResult<GetHooksResponse> {
    Ok(GetHooksResponse {
        hooks: HOOKS.query_hooks(deps)?.hooks,
    })
}

pub fn query_list_stakers(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    let start_at = start_after
        .map(|addr| deps.api.addr_validate(&addr))
        .transpose()?;

    let stakers = cw_paginate::paginate_snapshot_map(
        deps,
        &STAKED_BALANCES,
        start_at.as_ref(),
        limit,
        cosmwasm_std::Order::Ascending,
    )?;

    let stakers = stakers
        .into_iter()
        .map(|(address, balance)| StakerBalanceResponse {
            address: address.into_string(),
            balance,
        })
        .collect();

    to_binary(&ListStakersResponse { stakers })
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
    seed_denom: String,
    gamm_denom: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    // only manager can execute this
    if Some(info.sender) != config.manager {
        return Err(ContractError::Unauthorized {});
    }
    // create wasm execute message for ejecting liquidity
    let eject_msg = ExecuteMsg::EjectLiquidity {
        gamm_denom: gamm_denom.clone(),
    };
    let eject_msg = to_binary(&eject_msg)?;
    let eject_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: eject_msg,
    });
    let seed_msg = ExecuteMsg::SeedLiquidity {
        gamm_denom,
        seed_denom,
    };
    let seed_msg = to_binary(&seed_msg)?;
    let seed_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: info.funds,
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
    seed_denom: String,
    gamm_denom: String,
) -> Result<Response, ContractError> {
    // only allow the contract itself to execute this
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    let seed_amount = must_pay(&info, &seed_denom).map_err(|e| ContractError::PaymentError(e))?;
    let pool_id = extract_pool_id_from_denom(&gamm_denom)?;
    let gamm_querier = GammQuerier::new(&deps.querier);
    let pool: QueryTotalPoolLiquidityResponse = gamm_querier.total_pool_liquidity(pool_id)?;
    let asset_count = pool.liquidity.len();
    let seed_amount_per_pool = seed_amount / Uint128::from(asset_count as u128);
    let seed_amount_remainder =
        seed_amount - (seed_amount_per_pool * Uint128::from(asset_count as u128));

    let mut msgs: Vec<CosmosMsg> = vec![];
    let mut pool_creation_fees_to_collect = Uint128::from(asset_count.clone() as u64);
    let pool_creation_fee = Uint128::from(100000000u128);
    let mut osmo_fees_remaining = pool_creation_fee.mul(pool_creation_fees_to_collect);
    for coin in pool.liquidity {
        let balance = deps
            .querier
            .query_balance(&env.contract.address, &coin.denom)?;
        let balance = if coin.denom == "uosmo" {
            // subtract pool creation fee
            let bal = balance
                .amount
                .checked_sub(osmo_fees_remaining)
                .map_err(StdError::overflow)?;
            osmo_fees_remaining = osmo_fees_remaining
                .checked_sub(pool_creation_fee)
                .map_err(StdError::overflow)?;
            bal
        } else {
            balance.amount.clone()
        };
        pool_creation_fees_to_collect = pool_creation_fees_to_collect - Uint128::from(1u128);
        let msg_create_balancer_pool: CosmosMsg = MsgCreateBalancerPool {
            sender: env.contract.address.to_string(),
            future_pool_governor: "24h".to_string(),
            pool_params: Some(PoolParams {
                swap_fee: "3000000000000000".to_string(),
                exit_fee: "0".to_string(),
                smooth_weight_change_params: None,
            }),
            pool_assets: vec![
                PoolAsset {
                    token: Some(Coin {
                        denom: coin.denom.clone(),
                        amount: balance.to_string(),
                    }),
                    weight: "100".to_string(),
                },
                PoolAsset {
                    token: Some(Coin {
                        denom: seed_denom.to_string(),
                        amount: seed_amount_per_pool.to_string(),
                    }),
                    weight: "100".to_string(),
                },
            ],
        }
        .into();
        msgs.push(msg_create_balancer_pool);
    }

    let bank_transfer_remainder_msgs: Vec<CosmosMsg<Empty>> = if seed_amount_remainder.is_zero() {
        vec![]
    } else {
        vec![CosmosMsg::Bank(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: coins(seed_amount_remainder.u128(), seed_denom),
        })]
    };

    Ok(Response::new()
        .add_attribute("action", "execute_eject_and_seed_liquidity")
        .add_messages(msgs)
        .add_messages(bank_transfer_remainder_msgs))
}

pub fn _execute_eject(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    gamm_denom: String,
) -> Result<Response, ContractError> {
    // only allow the contract itself to execute this
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    let pool_id = extract_pool_id_from_denom(&gamm_denom)?;
    let balance = deps
        .querier
        .query_balance(&env.contract.address, gamm_denom)?;
    let gamm_querier = GammQuerier::new(&deps.querier);
    let pool_liquidity = gamm_querier.total_pool_liquidity(pool_id)?;
    let total_shares = gamm_querier.total_shares(pool_id)?;
    let total_shares_amount = total_shares
        .total_shares
        .clone()
        .ok_or(StdError::generic_err("failed to load "))?
        .amount
        .clone();
    let token_out_mins = pool_liquidity
        .liquidity
        .iter()
        .map(|coin| -> Result<Coin, StdError> {
            let token_out_min = Uint128::from(
                Uint128::from_str(&coin.amount)?
                    .multiply_ratio(
                        balance.amount.u128(),
                        Uint128::from_str(&total_shares_amount)?,
                    )
                    .u128(),
            );
            Ok(Coin {
                denom: coin.denom.clone(),
                amount: token_out_min.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let msg_exit_pool: CosmosMsg = MsgExitPool {
        sender: env.contract.address.to_string(),
        pool_id,
        share_in_amount: balance.amount.to_string(),
        token_out_mins: token_out_mins,
    }
    .into();
    Ok(Response::new().add_message(msg_exit_pool))
}

pub fn execute_distribute_all_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let bank_balances = deps.querier.query_all_balances(&env.contract.address)?;
    let mut msgs: Vec<CosmosMsg> = vec![];
    let config = CONFIG.load(deps.storage)?;
    for coin in bank_balances {
        // only distribute external tokens
        if coin.denom == config.denom {
            continue;
        }
        let distribute_token_wasm_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::DistributeToken { denom: coin.denom })?,
        });
        msgs.push(distribute_token_wasm_msg);
    }
    Ok(Response::new()
        .add_attribute("action", "execute_distribute_all_tokens")
        .add_messages(msgs))
}

pub fn execute_distribute_token(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
) -> Result<Response, ContractError> {
    // only allow the contract itself to execute this
    let config = CONFIG.load(deps.storage)?;
    let config_denom = config.denom.clone();
    if denom == config_denom {
        // cannot distribute the config token
        // can later consider allowing this as a means by which to terminate the pools
        return Err(ContractError::Unauthorized {});
    }
    let msgs: Vec<SubMsg> = if !REWARD_CONTRACTS_BY_DENOM.has(deps.storage, &denom) {
        // cannot distribute a token that is already being distributed
        let instantiate_lockdrop_rewards_msg: SubMsg<Empty> = SubMsg::reply_on_success(
            CosmosMsg::Wasm(WasmMsg::Instantiate {
                code_id: config.reward_contract_code_id,
                admin: Some(env.contract.address.to_string()),
                label: format!("lockdrop_rewards_{}", denom),
                msg: to_binary(&lockdrop_rewards::msg::InstantiateMsg {
                    owner: Some(env.contract.address.to_string()),
                    manager: Some(env.contract.address.to_string()),
                    staking_contract: env.contract.address.to_string(),
                    reward_token: Denom::Native(denom.clone()),
                    reward_duration: 24,
                })?,
                funds: vec![],
            }),
            1,
        );
        vec![instantiate_lockdrop_rewards_msg]
    } else {
        vec![]
    };

    Ok(Response::new()
        // submessages are executed first regardless of order here
        .add_submessages(msgs)
        .add_attribute("action", "distribute_token"))
}
