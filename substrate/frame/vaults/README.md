# Vaults Pallet

A Collateralized Debt Position (CDP) system for creating over-collateralized stablecoin loans on Substrate-based blockchains.

## Overview

The Vault pallet allows users to lock up collateral (DOT) and mint a stablecoin (pUSD) against it. This creates a decentralized lending system where:

- **Collateral is held**: Users deposit native tokens (DOT) which are held via `MutateHold`
- **Debt is minted**: Users can mint stablecoin (pUSD) up to a specified collateralization ratio
- **Interest accrues**: Vaults accumulate interest over time (stability fee)
- **Liquidation protects the system**: Under-collateralized vaults can be liquidated to maintain system solvency

**Key Design Choice**: Each account can have at most one vault. Collateral is held in the user's account (not transferred to a pallet account) using the `VaultDeposit` hold reason.

## Vault Lifecycle

### 1. Create Vault
```rust
create_vault(origin, initial_deposit)
```
- Creates a new vault with an initial collateral deposit
- Requires `initial_deposit >= MinimumDeposit`
- Collateral is held via `MutateHold::hold()` with `VaultDeposit` reason
- Vault starts with zero debt

### 2. Deposit More Collateral
```rust
deposit_collateral(origin, amount)
```
- Add more collateral to improve collateralization ratio
- Triggers fee accrual before deposit
- Useful before minting more debt or avoiding liquidation

### 3. Mint Stablecoin (Borrow)
```rust
mint(origin, amount)
```
- Mint stablecoin (pUSD) against locked collateral
- Requires `amount >= MinimumMint` (prevents dust mints)
- Must maintain the Initial Collateralization Ratio (higher than minimum for safety buffer)
- Verifies oracle price is fresh (not older than `OracleStalenessThreshold`)
- System enforces `MaximumIssuance` debt ceiling

### 4. Repay Debt
```rust
repay(origin, amount)
```
- Burn stablecoin to reduce debt
- Payment order: interest first (transferred to Insurance Fund), then principal (burned)
- Excess amount beyond total debt is not consumed

### 5. Withdraw Collateral
```rust
withdraw_collateral(origin, amount)
```
- Release held collateral back to owner
- Must maintain Initial Collateralization Ratio if vault has debt
- Verifies oracle price is fresh when vault has debt
- Cannot create dust vaults (remaining collateral must be >= MinimumDeposit or zero)

### 6. Close Vault
```rust
close_vault(origin)
```
- Close a debt-free vault and release all collateral
- Requires zero debt (all loans repaid)
- Transfers any accrued interest to Insurance Fund before closing
- Removes vault from storage

### 7. Poke (Force Fee Accrual)
```rust
poke(origin, vault_owner)
```
- Permissionless extrinsic to force fee accrual on any vault
- Useful for keeping vault state fresh for accurate queries
- Cannot poke a vault that is `InLiquidation`

### 8. Liquidation (Called by Keepers)
```rust
liquidate_vault(origin, vault_owner)
```
- Anyone can liquidate vaults below Minimum Collateralization Ratio
- Verifies oracle price is fresh
- Calculates liquidation penalty on principal
- Changes hold reason from `VaultDeposit` to `Seized`
- Starts auction via `AuctionsHandler`

**Note**: Protocol revenue comes solely from stability fees. Liquidation penalties incentivize external keepers to monitor and liquidate risky vaults promptly.

## Liquidation

The pallet implements liquidation risk management via concurrent auction limits.

### MaxLiquidationAmount and CurrentLiquidationAmount

- **MaxLiquidationAmount**: Hard limit on pUSD at risk in active auctions (set via governance)
- **CurrentLiquidationAmount**: Current pUSD at risk across all active auctions (accumulator)

When liquidating, the system checks if adding the new auction's debt would exceed `MaxLiquidationAmount`. If so, the liquidation is blocked with `ExceedsMaxLiquidationAmount` error.

This is a **hard limit** to prevent too much collateral being auctioned simultaneously, which could overwhelm market liquidity.

### Auction Integration

The Auctions pallet communicates back to Vaults via the `CollateralManager` trait:

```rust
pub trait CollateralManager<AccountId> {
    type Balance;

    /// Get current collateral price from oracle.
    fn get_dot_price() -> Option<FixedU128>;

    /// Execute a purchase: collect pUSD from buyer, transfer collateral to recipient.
    fn execute_purchase(params: PurchaseParams<AccountId, Self::Balance>) -> DispatchResult;

    /// Complete an auction: return excess collateral and record any shortfall.
    fn complete_auction(
        vault_owner: &AccountId,
        remaining_collateral: Self::Balance,
        shortfall: Self::Balance,
    ) -> DispatchResult;
}
```

### Bad Debt

When auctions can't cover the full debt, the shortfall is recorded as `BadDebt`. This represents a system deficit that can be addressed via:

```rust
heal(origin, amount)
```
- Permissionless extrinsic to burn pUSD from the Insurance Fund to reduce bad debt

## Interest Accrual

Vaults accumulate interest (stability fee) over time using timestamps:

```
Interest_pUSD = Principal × StabilityFee × (DeltaMillis / MillisPerYear)
```

Where:
- `DeltaMillis = current_timestamp - last_fee_update`
- `MillisPerYear = 31,557,600,000` (365.25 days)

Interest is stored in `accrued_interest` and transferred to the Insurance Fund during repayment or vault closure.

### Stale Vault Housekeeping

During `on_idle`, the pallet updates fees for stale vaults:
- A vault is stale if `current_timestamp - last_fee_update >= StaleVaultThreshold`
- Uses cursor-based pagination across blocks
- Only updates `accrued_interest` - no transfers occur

## Collateralization Ratio

The system enforces two key ratios:

1. **Initial Collateralization Ratio** (e.g., 200%)
   - Required when minting new debt or withdrawing collateral
   - Ensures adequate buffer for price volatility

2. **Minimum Collateralization Ratio** (e.g., 180%)
   - Liquidation threshold
   - Vaults below this ratio can be liquidated

**Formula**:
```
CR = (collateral_amount × oracle_price) / (principal + accrued_interest)
```

## Oracle Integration

The pallet requires a price oracle that provides:
- **Normalized price**: `smallest_pUSD_units / smallest_collateral_unit`
- **Timestamp**: When the price was last updated

Operations are paused when the oracle price is older than `OracleStalenessThreshold`:
- `mint` fails with `OracleStale`
- `withdraw_collateral` (with debt) fails with `OracleStale`
- `liquidate_vault` fails with `OracleStale`

## Configuration

The pallet requires the following configuration in the runtime:

```rust
impl pallet_vaults::Config for Runtime {
    type Currency = Balances;                    // Native token for collateral (MutateHold)
    type RuntimeHoldReason = RuntimeHoldReason;  // Hold reason enum
    type Asset = Assets;                         // Multi-asset pallet for stablecoin
    type AssetId = u32;
    type StablecoinAssetId = StablecoinAssetId;  // Constant: ID of pUSD
    type InsuranceFund = InsuranceFund;          // Account receiving protocol revenue
    type MinimumDeposit = MinimumDeposit;        // Min collateral to create vault
    type MinimumMint = MinimumMint;              // Min pUSD per mint operation
    type TimeProvider = Timestamp;               // For timestamp-based fees
    type StaleVaultThreshold = StaleVaultThreshold;      // When vaults need on_idle update
    type OracleStalenessThreshold = OracleStalenessThreshold;  // Max oracle price age
    type Oracle = NudgeAggregator;               // Price oracle (ProvidePrice trait)
    type CollateralLocation = CollateralLocation; // Location for oracle queries
    type AuctionsHandler = Auctions;             // Liquidation handler
    type ManagerOrigin = EnsureVaultsManager;    // Governance origin (returns privilege level)
    type WeightInfo = weights::SubstrateWeight<Runtime>;
}
```

### Required Constants

- `StablecoinAssetId`: The asset ID for the minted stablecoin (pUSD)
- `InsuranceFund`: Account that receives collected interest and penalties
- `MinimumDeposit`: Minimum DOT to create a vault (prevents dust)
- `MinimumMint`: Minimum pUSD per mint (prevents dust)
- `StaleVaultThreshold`: Milliseconds before vault is considered stale (default: 4 hours)
- `OracleStalenessThreshold`: Max oracle price age before operations pause (default: 1 hour)
- `CollateralLocation`: XCM Location identifying collateral for oracle

### Parameters (Set via Governance)

| Parameter | Description | Example |
|-----------|-------------|---------|
| `MinimumCollateralizationRatio` | Liquidation threshold | 180% |
| `InitialCollateralizationRatio` | Required for minting/withdrawing | 200% |
| `StabilityFee` | Annual interest rate | 4% |
| `LiquidationPenalty` | Penalty on liquidated principal | 13% |
| `MaximumIssuance` | System-wide debt ceiling | 20M pUSD |
| `MaxLiquidationAmount` | Max pUSD at risk in auctions | 20M pUSD |

### Privilege Levels

The `ManagerOrigin` returns a privilege level:
- **Full** (via GeneralAdmin): Can modify all parameters
- **Emergency** (via EmergencyAction): Can only lower debt ceiling (defensive action)

## Events

- `VaultCreated { owner }`: New vault created
- `CollateralDeposited { owner, amount }`: Collateral added to vault
- `CollateralWithdrawn { owner, amount }`: Collateral removed from vault
- `Minted { owner, amount }`: Stablecoin minted (debt increased)
- `Repaid { owner, amount }`: Principal repaid and burned
- `ReturnedExcess { owner, amount }`: Excess pUSD when repayment exceeded debt
- `InterestCollected { owner, amount }`: Interest transferred to Insurance Fund
- `InterestUpdated { owner, amount }`: Interest accrued (during fee update)
- `VaultClosed { owner }`: Vault closed and removed
- `InLiquidation { owner, debt, collateral_seized }`: Vault liquidated
- `LiquidationPenaltyAdded { owner, amount }`: Liquidation penalty applied
- `AuctionStarted { owner, auction_id, collateral, tab }`: Auction initiated
- `AuctionDebtCollected { amount }`: pUSD collected from auction
- `AuctionShortfall { shortfall }`: Auction couldn't cover debt
- `BadDebtAccrued { owner, amount }`: Debt exceeded collateral
- `BadDebtRepaid { amount }`: Bad debt reduced via heal
- `MinimumCollateralizationRatioUpdated { old_value, new_value }`
- `InitialCollateralizationRatioUpdated { old_value, new_value }`
- `StabilityFeeUpdated { old_value, new_value }`
- `LiquidationPenaltyUpdated { old_value, new_value }`
- `MaximumIssuanceUpdated { old_value, new_value }`
- `MaxLiquidationAmountUpdated { old_value, new_value }`

## Errors

- `VaultNotFound`: No vault exists for the account
- `VaultAlreadyExists`: Account already has a vault
- `VaultHasDebt`: Cannot close vault with outstanding debt
- `VaultIsSafe`: Cannot liquidate a healthy vault
- `VaultInLiquidation`: Vault is in liquidation; operations blocked
- `InsufficientCollateral`: Not enough collateral for operation
- `UnsafeCollateralizationRatio`: Operation would breach required CR
- `ExceedsMaxDebt`: Minting would exceed system debt ceiling
- `ExceedsMaxLiquidationAmount`: Liquidation would exceed auction limit
- `BelowMinimumDeposit`: Deposit or remaining collateral too small
- `BelowMinimumMint`: Mint amount below minimum
- `PriceNotAvailable`: Oracle returned no price
- `OracleStale`: Oracle price is too old
- `ArithmeticOverflow`: Calculation overflow
- `InsufficientPrivilege`: Emergency tried Full-only operation
- `CanOnlyLowerMaxDebt`: Emergency tried to raise debt ceiling
- `InitialRatioMustExceedMinimum`: ICR must be >= MCR

## Testing

Run tests with:
```bash
SKIP_WASM_BUILD=1 cargo test -p pallet-vaults
```
