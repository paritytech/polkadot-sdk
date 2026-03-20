# PSM Pallet

A Peg Stability Module enabling 1:1 swaps between pUSD and pre-approved external stablecoins on Substrate-based blockchains.

## Overview

The PSM pallet allows users to swap external stablecoins (e.g., USDC, USDT) for pUSD and vice versa at a 1:1 rate (minus fees). This creates a decentralized peg stabilization mechanism where:

- **Reserves are held**: External stablecoins are held in a pallet-derived account (`PalletId`)
- **pUSD is minted/burned**: Users receive pUSD when depositing external stablecoins, and burn pUSD when redeeming
- **Circuit breaker provides emergency control**: Per-asset circuit breaker can disable minting or all swaps

## Swap Lifecycle

### 1. Mint (External -> pUSD)
```rust
mint(origin, asset_id, external_amount)
```
- Deposits external stablecoin into the PSM account
- Mints pUSD to the user (minus minting fee)
- Fee is minted as pUSD to the Insurance Fund
- Enforces three-tier debt ceiling: system-wide, aggregate PSM, and per-asset
- Requires `external_amount >= MinSwapAmount`

### 2. Redeem (pUSD -> External)
```rust
redeem(origin, asset_id, pusd_amount)
```
- Burns pUSD from the user (minus redemption fee)
- Transfers external stablecoin from PSM account to user
- Fee is transferred as pUSD from user to Insurance Fund
- Limited by tracked PSM debt (not raw reserve balance)
- Requires `pusd_amount >= MinSwapAmount`

## Debt Ceiling Architecture

Before minting, the PSM checks three ceilings in order:

1. **System-wide**: `total_issuance(pUSD) + amount <= MaximumIssuance`
2. **Aggregate PSM**: `total_psm_debt + amount <= MaxPsmDebtOfTotal * MaximumIssuance`
3. **Per-asset**: `asset_debt + amount <= normalized_asset_share_of_psm_ceiling`

### PSM Reserved Capacity

The PSM's allocation is guaranteed via the `PsmInterface` trait. The Vaults pallet queries `reserved_capacity()` and enforces an effective vault ceiling of `MaximumIssuance - reserved_capacity()`, preventing vaults from consuming PSM's share.

### Per-Asset Ceiling

Per-asset ceilings use a weight-based system:

```
max_asset_debt = (AssetCeilingWeight[asset_id] / sum_of_all_weights) * max_psm_debt
```

Setting an asset's weight to 0% disables minting and redistributes its capacity to other assets.

## Fee Structure

Fees are calculated using `Permill::mul_ceil` (rounds up):

- **Minting Fee**: `fee = MintingFee[asset_id].mul_ceil(external_amount)` -- deducted from pUSD output, minted to Insurance Fund
- **Redemption Fee**: `fee = RedemptionFee[asset_id].mul_ceil(pusd_amount)` -- transferred as pUSD from user to Insurance Fund

With 0.5% fees on both sides, arbitrage opportunities exist when pUSD trades outside $0.995-$1.005.

## Circuit Breaker

Each approved asset has an independent circuit breaker with three levels:

| Level             | Minting | Redemption | Use Case                          |
| ----------------- | ------- | ---------- | --------------------------------- |
| `AllEnabled`      | Allowed | Allowed    | Normal operation                  |
| `MintingDisabled` | Blocked | Allowed    | Drain debt from problematic asset |
| `AllDisabled`     | Blocked | Blocked    | Full emergency halt               |

The `set_asset_status` extrinsic can be called by both `GeneralAdmin` and `EmergencyAction` origins.

## Governance Operations

| Extrinsic                                    | Required Level    | Description                                       |
| -------------------------------------------- | ----------------- | ------------------------------------------------- |
| `set_minting_fee(asset_id, fee)`             | Full              | Update minting fee for an asset                   |
| `set_redemption_fee(asset_id, fee)`          | Full              | Update redemption fee for an asset                |
| `set_max_psm_debt(ratio)`                    | Full              | Update global PSM ceiling as % of MaximumIssuance |
| `set_asset_ceiling_weight(asset_id, weight)` | Full              | Update per-asset ceiling weight                   |
| `set_asset_status(asset_id, status)`         | Full or Emergency | Set per-asset circuit breaker level               |
| `add_external_asset(asset_id)`               | Full              | Add approved stablecoin (defaults to AllEnabled)  |
| `remove_external_asset(asset_id)`            | Full              | Remove approved stablecoin (requires zero debt)   |

### Privilege Levels

The `ManagerOrigin` returns a privilege level:
- **Full** (via GeneralAdmin): Can modify all parameters
- **Emergency** (via EmergencyAction): Can only modify circuit breaker status

### Asset Offboarding Workflow

1. `set_asset_ceiling_weight(asset_id, 0%)` -- blocks minting, redistributes capacity
2. Redemptions slowly drain remaining PSM debt
3. Once `PsmDebt[asset_id]` reaches zero, call `remove_external_asset(asset_id)`

## Configuration

```rust
impl pallet_psm::Config for Runtime {
    type Asset = Assets;                    // Fungibles impl for pUSD and external stablecoins
    type AssetId = u32;                     // Asset identifier type
    type VaultsInterface = Vaults;          // Interface to query MaximumIssuance from Vaults
    type ManagerOrigin = EnsurePsmManager;  // Governance origin (returns privilege level)
    type WeightInfo = weights::SubstrateWeight<Runtime>;
    type StablecoinAssetId = StablecoinAssetId;  // Constant: pUSD asset ID
    type InsuranceFund = InsuranceFundAccount;    // Account receiving fee revenue
    type PalletId = PsmPalletId;                  // For deriving PSM account address
    type MinSwapAmount = MinSwapAmount;           // Minimum swap amount (prevents dust)
}
```

### Parameters (Set via Governance)

| Parameter            | Description                          | Suggested Value       |
| -------------------- | ------------------------------------ | --------------------- |
| `MaxPsmDebtOfTotal`  | PSM ceiling as % of MaximumIssuance  | 10%                   |
| `MintingFee`         | Fee for external -> pUSD (per asset) | 0.5%                  |
| `RedemptionFee`      | Fee for pUSD -> external (per asset) | 0.5%                  |
| `AssetCeilingWeight` | Per-asset share of PSM ceiling       | 50% each (USDC, USDT) |

### Required Constants

- `StablecoinAssetId`: The asset ID for pUSD
- `InsuranceFund`: Account that receives fee revenue (shared with pallet-vaults)
- `PalletId`: Unique identifier for deriving the PSM account
- `MinSwapAmount`: Minimum amount for any swap (default: 100 pUSD)

## Events

- `Minted { who, asset_id, external_amount, pusd_received, fee }`: User swapped external stablecoin for pUSD
- `Redeemed { who, asset_id, pusd_paid, external_received, fee }`: User swapped pUSD for external stablecoin
- `MintingFeeUpdated { asset_id, old_value, new_value }`: Minting fee changed
- `RedemptionFeeUpdated { asset_id, old_value, new_value }`: Redemption fee changed
- `MaxPsmDebtOfTotalUpdated { old_value, new_value }`: Global PSM ceiling changed
- `AssetCeilingWeightUpdated { asset_id, old_value, new_value }`: Per-asset ceiling weight changed
- `AssetStatusUpdated { asset_id, status }`: Circuit breaker level changed
- `ExternalAssetAdded { asset_id }`: New external stablecoin approved
- `ExternalAssetRemoved { asset_id }`: External stablecoin removed

## Errors

- `UnsupportedAsset`: Asset is not in the approved list
- `InsufficientReserve`: PSM doesn't have enough external stablecoin for redemption
- `ExceedsMaxIssuance`: Mint would exceed system-wide pUSD cap
- `ExceedsMaxPsmDebt`: Mint would exceed aggregate PSM ceiling or per-asset ceiling
- `BelowMinimumSwap`: Swap amount below MinSwapAmount
- `MintingStopped`: Minting disabled by circuit breaker
- `AllSwapsStopped`: All swaps disabled by circuit breaker
- `AssetAlreadyApproved`: Asset already in approved list
- `AssetNotApproved`: Asset not in approved list
- `AssetHasDebt`: Cannot remove asset with outstanding debt
- `InsufficientPrivilege`: Emergency origin tried a Full-only operation

## Testing

Run tests with:
```bash
SKIP_WASM_BUILD=1 cargo test -p pallet-psm
```
