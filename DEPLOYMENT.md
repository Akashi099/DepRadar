# Deployment Guide

This document describes how to deploy and upgrade the `deprader_router` contract on Stellar.

## Initial Deployment

1. Build the optimized WASM:
```bash
cargo build --target wasm32-unknown-unknown --release --package deprader_router
```

2. Optimize the WASM (optional but recommended):
```bash
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/deprader_router.wasm \
  --wasm-out deprader_router_optimized.wasm
```

3. Deploy the contract:
```bash
stellar contract deploy \
  --wasm deprader_router_optimized.wasm \
  --source <YOUR_FUNDED_ACCOUNT> \
  --network testnet
```

This returns the contract ID (e.g., `CCPR...YQJQ`).

4. Initialize the contract with an admin address:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network testnet \
  -- initialize --admin <ADMIN_ADDRESS>
```

## Contract Upgrade Procedure

The `upgrade` function allows the admin to update the contract to new WASM without losing state or changing the contract ID.

### Prerequisites
- Admin account with signing authority
- New contract WASM built and optimized
- Contract must already be deployed and initialized

### Upgrade Steps

1. Build the new contract version:
```bash
cargo build --target wasm32-unknown-unknown --release --package deprader_router
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/deprader_router.wasm \
  --wasm-out deprader_router_v2.wasm
```

2. Install (upload) the new WASM to the network and get its hash:
```bash
stellar contract install \
  --wasm deprader_router_v2.wasm \
  --source <ADMIN_ACCOUNT> \
  --network testnet
```

This returns a 32-byte hash like `a1b2c3d4...`.

3. Invoke the `upgrade` function with the new WASM hash:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network testnet \
  -- upgrade --new_wasm_hash <WASM_HASH>
```

4. Verify the upgrade succeeded:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ANY_ACCOUNT> \
  --network testnet \
  -- get_admin
```

If this returns the expected admin address, the upgrade succeeded and all state is preserved.

### Important Notes

- **State Preservation**: All bounties, claims, and admin settings remain intact after an upgrade. Only the contract code changes.
- **Contract ID**: The contract ID remains the same. Clients do not need to update their contract addresses.
- **Admin-Only**: Only the current admin can invoke `upgrade`. The call will fail for any other caller.
- **Testing**: Always test upgrades on testnet before applying them to mainnet.
- **Events**: The upgrade emits an `upgraded` event with the admin address and new WASM hash, which can be monitored for audit purposes.
- **Emergency Use**: The upgrade function can be used during the pause state if critical fixes are needed.

### Rollback

If an upgrade introduces issues, the admin can "roll back" by uploading the previous contract WASM and calling `upgrade` again with its hash. Since state is preserved, this restores the old behavior without data loss.

## Network Endpoints

- **Testnet RPC**: `https://soroban-testnet.stellar.org`
- **Mainnet RPC**: `https://soroban-mainnet.stellar.org`

## Useful Commands

Check contract admin:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- get_admin
```

Check pause state:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- is_paused
```

Get bounty count:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- bounty_count
```
