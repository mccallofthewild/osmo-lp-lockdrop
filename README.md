# Contracts 

## `native-stake`
Implements the DAODAO cw20-stake interfaces for native CosmosSDK tokens. 20+ unit tests. Positioned to be utilized for DAODAO v1/v2 upgrades on Osmosis. Supports autocompounding rewards, external reward contracts, and other cw20-stake compatible contracts.

## `osmo-lp-lockdrop`
The Osmosis LP Lockdrop is a fork of `native-stake` which enables the utilization of LP tokens for seed-style investing. When the seed token launches, the backing team (`config.manager`). Utilizes `osmosis-rust` to interact with the Osmosis native DEX layer.

### Process
1. LP Lockdrop Contract is Instantiated with a GAMM denom
   1. transfer 100000000uosmo per underlying pool asset to the contract
      1. Used for pool creation fees
   2. registers reward distributor contract id 
   3. defines GAMM denom which can be staked
2. Execute `Stake {}`, passing GAMM token in `funds[]`
   1. by default, this locks tokens up for 365 days 
3. Execute `EjectAndSeedLiquidity`
   1. Passing total initial seed token liquidity in `funds[]`
   2. Exit Pool, withdrawing all LP tokens to their underlying representations 
   3. Divide seed token amount accordingly between the underlying assets 
   4. Create new pools, matching seed token with each asset 

### Known Issues
1. Currently susceptible to sandwich attacks. This can be fixed by comparing pool distribution to TWAP.
2. Asymmetrically weighted pools are not supported.


# Actions
* Instantiate 
  * manager 
  * Name
  * GAMM denom 
* Stake GAMM
* Unstake GAMM
* Seed Liquidity (manager only)
  * `denom: newtoken`
  * `funds: [100newtoken]`
  * ...
    * Remove liquidity (Convert GAMM to derivative tokens)
    * Query denoms associated with pool GAMM denom
    * Create tripool or two pools with half of the funds for each 
      * if you split in half, there is a sandwich attack that could make one half considerably cheaper than it should be 
      * Maybe use TWAP to minimize this risk
    * creates a new pool
    * sends LP tokens to cw20-stake-external-rewards

# Further Thoughts
* Manager misbehavior 
  * Connect to DAODAO contract to enable participants to vote to remove lockup if project fails to deliver.
  * Add Sudo message to remove lockup if project fails to deliver.

```typescript
client.stake(
  [
    {
      denom: "uosmo",
      amount: "1",
    },
  ],
)

client.get_config();
// javscript
let promise = new Promise();

// 
cosmwasm_client.instantiate(code_id, )
```

