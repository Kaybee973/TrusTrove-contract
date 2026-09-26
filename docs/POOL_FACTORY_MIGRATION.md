# Pool Factory Migration Guide

This document describes the migration path for the existing deployed testnet pool contract to become "instance #1" under the new pool_factory, avoiding a disruptive redeploy that would orphan existing LP positions.

## Background

The current testnet deployment has a single USDC pool contract that predates the factory pattern. To support multiple assets via the new pool_factory, we need to register this existing pool as the first instance without requiring LPs to withdraw and redeposit.

## Migration Steps

### 1. Deploy pool_factory

Deploy the new pool_factory contract following the standard deployment process in `DEPLOYMENT.md`.

### 2. Register the existing pool

The factory will need a `register_existing_pool(asset, pool_address)` function that:

- Takes an asset address and an already-deployed pool contract address
- Skips the deploy-and-initialize step (since the pool already exists)
- Records the mapping: `DataKey::AssetToPool(asset) = pool_address`
- Updates the asset index: increments `DataKey::AssetCount` and appends to `DataKey::AssetIndex`

This variant is scoped as a follow-up issue. Once implemented, the migration command would be:

```bash
stellar contract invoke \
  --id <factory_contract_id> \
  --w <deployer_wallet> \
  register_existing_pool \
  --asset <usdc_asset_address> \
  --pool_address <existing_pool_contract_id>
```

### 3. Verify registration

Call `list_assets()` on the factory to confirm the USDC asset is registered:

```bash
stellar contract invoke \
  --id <factory_contract_id> \
  list_assets
```

The response should include the USDC asset address.

### 4. Verify pool lookup

Call `get_pool_for_asset(asset)` (once implemented) to confirm the factory returns the existing pool address:

```bash
stellar contract invoke \
  --id <factory_contract_id> \
  get_pool_for_asset \
  --asset <usdc_asset_address>
```

## Follow-up Requirements

The `register_existing_pool` function has been implemented in this PR. This function:

- Accepts `asset: Address` and `pool_address: Address` parameters
- Requires admin authorization
- Checks that the asset is not already registered
- Updates the same storage keys as `register_asset` would, but skips deployment
- Updates the asset index and asset-to-pool mapping

## Benefits

This migration approach:

- Preserves all existing LP positions and shares
- Avoids requiring LPs to withdraw and redeposit
- Maintains the pool's existing state and liquidity
- Allows seamless integration with the new factory pattern
- Enables future asset registrations via the standard `register_asset` flow
