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

