# PSM Pallet

A Peg Stability Module enabling 1:1 swaps between pUSD and pre-approved
external stablecoins on Substrate-based blockchains.

## Overview

The PSM pallet allows users to swap external stablecoins (e.g., USDC, USDT)
for pUSD and vice versa at a 1:1 rate (minus fees). This creates a
decentralized peg stabilization mechanism where:

- **Reserves are held**: External stablecoins are held in a pallet-derived account (`PalletId`)
- **pUSD is minted/burned**: Users receive pUSD when depositing external stablecoins, and burn pUSD when redeeming
- **Fees are routed to `FeeDestination`**: Mint and redeem fees are
  collected in pUSD and transferred to a configurable account
- **Circuit breaker provides emergency control**: Per-asset circuit breaker can disable minting or all swaps

## Swap Lifecycle

### 1. Mint (External -> pUSD)

```rust
mint(origin, asset_id, external_amount)
```

- Deposits external stablecoin into the PSM account
- Mints pUSD to the user (minus minting fee)
- Fee is minted as pUSD and transferred to `FeeDestination`
- Enforces three-tier debt ceiling: system-wide, aggregate PSM, and per-asset
- Requires `external_amount >= MinSwapAmount`

### 2. Redeem (pUSD -> External)

```rust
redeem(origin, asset_id, pusd_amount)
```

- Burns pUSD from the user equal to the external amount being redeemed
- Transfers external stablecoin from PSM account to user
- Redemption fee is transferred from the user as pUSD to `FeeDestination`
- Limited by tracked PSM debt (not raw reserve balance)
- Requires `pusd_amount >= MinSwapAmount`

## Debt Ceiling Architecture

Before minting, the PSM checks three ceilings in order:

1. **System-wide**: `total_issuance(pUSD) + amount <= MaximumIssuance`
2. **Aggregate PSM**: `total_psm_debt + amount <= MaxPsmDebtOfTotal * MaximumIssuance`
3. **Per-asset**: `asset_debt + amount <= normalized_asset_share_of_psm_ceiling`

### PSM Reserved Capacity

The PSM's allocation is guaranteed via the `PsmInterface` trait.
The Vaults pallet queries `reserved_capacity()` and enforces an effective
vault ceiling of `MaximumIssuance - reserved_capacity()`, preventing vaults
from consuming PSM's share.

### Per-Asset Ceiling

Per-asset ceilings use a weight-based system:

```
max_asset_debt = (AssetCeilingWeight[asset_id] / sum_of_all_weights) * max_psm_debt
```

Setting an asset's weight to 0% disables minting and redistributes its capacity to other assets.

## Fee Structure

Fees are calculated using `Permill::mul_ceil` (rounds up) and transferred as pUSD to `FeeDestination`:

- **Minting Fee**: `fee = MintingFee[asset_id].mul_ceil(external_amount)`
  -- deducted from pUSD output, minted to `FeeDestination`
- **Redemption Fee**: `fee = RedemptionFee[asset_id].mul_ceil(pusd_amount)`
  -- transferred from the user to `FeeDestination`

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
| `add_external_asset(asset_id)`               | Full              | Add approved stablecoin (matching decimals)       |
| `remove_external_asset(asset_id)`            | Full              | Remove approved stablecoin (requires zero debt)   |

### Privilege Levels

The `ManagerOrigin` returns a privilege level:

- **Full** (via GeneralAdmin): Can modify all parameters
- **Emergency** (via EmergencyAction): Can only modify circuit breaker status

### Asset Offboarding Workflow

1. `set_asset_ceiling_weight(asset_id, 0%)` -- blocks minting, redistributes capacity
2. Redemptions slowly drain remaining PSM debt
3. Once `PsmDebt[asset_id]` reaches zero, call `remove_external_asset(asset_id)`

### Asset Onboarding Requirements

Before calling `add_external_asset(asset_id)`:

- The asset must already exist in the `Fungibles` implementation
- The asset's decimals must match `StableAsset::decimals()`
- The pallet must still be below `MaxExternalAssets`

## Configuration

```rust
impl pallet_psm::Config for Runtime {
    type Fungibles = Assets;
    type AssetId = u32;
    type MaximumIssuance = MaximumIssuance;
    type ManagerOrigin = EnsurePsmManager;
    type WeightInfo = weights::SubstrateWeight<Runtime>;
    type StableAsset = frame_support::traits::fungible::ItemOf<
        Assets,
        StablecoinAssetId,
        AccountId,
    >;
    type FeeDestination = InsuranceFundAccount;
    type PalletId = PsmPalletId;
    type MinSwapAmount = MinSwapAmount;
    type MaxExternalAssets = ConstU32<10>;
}
```

`Fungibles` must expose metadata for approved assets, and `StableAsset`
must expose metadata for the pUSD asset because `add_external_asset`
validates that decimals match before approval. `MaximumIssuance` provides
the system-wide pUSD cap (typically from the Vaults pallet or a constant).

### Parameters (Set via Governance)

| Parameter            | Description                          | Suggested Value       |
| -------------------- | ------------------------------------ | --------------------- |
| `MaxPsmDebtOfTotal`  | PSM ceiling as % of MaximumIssuance  | 10%                   |
| `MintingFee`         | Fee for external -> pUSD (per asset) | 0.5%                  |
| `RedemptionFee`      | Fee for pUSD -> external (per asset) | 0.5%                  |
| `AssetCeilingWeight` | Per-asset share of PSM ceiling       | 50% each (USDC, USDT) |

### Required Constants

- `PalletId`: Unique identifier for deriving the PSM account
- `MinSwapAmount`: Minimum amount for any swap (default: 100 pUSD)
- `MaxExternalAssets`: Maximum number of approved external assets

Typical runtime helpers used in the configuration above:

- `StablecoinAssetId`: Runtime constant used by `ItemOf<..., StablecoinAssetId, ...>` to bind `StableAsset` to pUSD
- `InsuranceFundAccount`: Account that receives pUSD fees via `FeeDestination`

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
- `TooManyAssets`: Maximum number of approved external assets reached
- `DecimalsMismatch`: External asset decimals do not match the stable asset decimals
- `Unexpected`: An unexpected invariant violation occurred (defensive check)

## Testing

Run tests with:

```bash
SKIP_WASM_BUILD=1 cargo test -p pallet-psm
```
