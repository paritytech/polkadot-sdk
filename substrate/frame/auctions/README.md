# Auctions Pallet

Dutch auction system for liquidating vault collateral and distributing protocol surplus on Substrate-based blockchains.

## Overview

The Auctions pallet implements MakerDAO Liquidation 2.0 style Dutch auctions for the pUSD protocol. It handles two auction types:

- **Liquidation Auctions**: Sell seized DOT collateral for pUSD to cover vault debt
- **Surplus Auctions**: Sell excess pUSD from the Insurance Fund for DOT (sent to Treasury)

**Key Design Choice**: Prices start high and decrease over time according to a configurable price curve. Buyers can purchase at the current price instantly - no bidding required.

## Auction Types

### Liquidation Auction

When a vault is liquidated by `pallet-vaults`, an auction is started to sell the seized DOT collateral:

- **Price**: pUSD per DOT (decreases over time)
- **Collateral**: DOT held with `HoldReason::Seized` on vault owner's account
- **Proceeds**: Principal burned, interest/penalty to Insurance Fund
- **Excess**: Remaining collateral returned to vault owner

### Surplus Auction

When the Insurance Fund accumulates excess pUSD (above threshold), surplus auctions distribute it:

- **Price**: DOT per pUSD (decreases over time)
- **Source**: pUSD from Insurance Fund
- **Proceeds**: DOT sent to Treasury
- **Limit**: Only one surplus auction active at a time

## Auction Lifecycle

### 1. Start (Liquidation)
```rust
// Called by pallet-vaults via AuctionsHandler trait
AuctionsHandler::start_auction(vault_owner, collateral, debt, keeper)
```
- Collateral already held with `Seized` reason by vaults pallet
- Initial price = oracle price × buffer (e.g., 20% above oracle)
- Keeper incentive calculated: `tip + chip × tab` (capped to penalty)

### 2. Start (Surplus)
```rust
start_surplus_auction(origin, keeper)
```
- Requires Insurance Fund balance > threshold × total pUSD supply
- Must be in `Auction` surplus mode (not `DirectTransfer`)
- Initial price = inverse oracle price × buffer

### 3. Take (Purchase)
```rust
// Liquidation: buyer pays pUSD, receives DOT
take_liquidation(origin, auction_id, dot_amount, max_pusd_per_dot, recipient)

// Surplus: buyer pays DOT, receives pUSD
take_surplus(origin, auction_id, pusd_amount, max_dot_per_pusd, recipient)
```
- Validates price against buyer's maximum
- Partial purchases allowed (dust prevention enforced)
- Instant settlement - no waiting period

### 4. Restart (Liquidation Only)
```rust
restart_auction(origin, auction_id, keeper)
```
- Required when auction exceeds `maximum_duration` or price falls below `minimum_price`
- Resets price to current oracle × buffer
- Updates keeper who will receive incentive at completion
- Surplus auctions simply end when stale (unsold pUSD stays in IF)

### 5. Completion
Auctions complete when:
- All collateral sold (liquidation) or all pUSD sold (surplus)
- Debt fully covered (liquidation)
- Auction removed from storage

On liquidation completion:
- Excess collateral returned to vault owner
- Keeper incentive paid from Insurance Fund (capped to penalty collected)
- Any shortfall recorded as bad debt

## Price Curve

The `SlowedExponentialDecrease` curve uses a cubic polynomial:

```
price = max(
    oracle_price × center_ratio - cubic_term - linear_term,
    starting_price × minimum_price
)
```

The curve:
- Starts above oracle price (buffer > 1)
- Inflects around the center block
- Decays faster far from center, slower near center
- Respects minimum price floor

### Parameters

| Parameter       | Description                           | Default |
| --------------- | ------------------------------------- | ------- |
| `center`        | Block where curve inflects            | 10      |
| `scale_factor`  | Cubic term divisor (higher = flatter) | 1000    |
| `linear_coeff`  | Linear decay rate                     | 0.0065  |
| `center_ratio`  | Price ratio at center                 | 0.99    |
| `minimum_price` | Floor as ratio of starting price      | 0.65    |

## Circuit Breaker

Four-level system for gradual shutdown:

| Level                     | New Auctions | Restarts | Takes |
| ------------------------- | ------------ | -------- | ----- |
| `AllEnabled`              | ✅            | ✅        | ✅     |
| `NoNewAuctions`           | ❌            | ✅        | ✅     |
| `NoNewAuctionsOrRestarts` | ❌            | ❌        | ✅     |
| `AllDisabled`             | ❌            | ❌        | ❌     |

Set via `set_stopped(origin, level)` by `ManagerOrigin`.

## Surplus Handling Modes

Governance can choose how surplus is distributed:

- **Auction**: `start_surplus_auction()` enabled - price discovery via Dutch auction
- **DirectTransfer**: `transfer_surplus()` enabled - direct transfer to Treasury

## Keeper Incentives

Keepers who start or restart auctions receive rewards:

```
incentive = tip + (chip × total_tab)
```

- Paid from Insurance Fund at auction completion
- Capped to actual penalty collected (prevents overpaying on shortfall)

## Tab (Structured Debt)

Auction debt is tracked with payment priority:

1. **Principal** (burned) - maintains pUSD peg
2. **Accrued Interest** (to Insurance Fund) - protocol revenue
3. **Penalty** (to Insurance Fund) - keeper incentive pool

## Configuration

```rust
impl pallet_auctions::Config for Runtime {
    type CollateralManager = Vaults;           // Oracle + collateral operations
    type MinAuctionTab = MinAuctionTab;        // Min debt to prevent dust
    type MinPurchaseAmount = MinPurchaseAmount; // Min DOT per liquidation purchase
    type MinSurplusPurchaseAmount = MinSurplusPurchaseAmount; // Min pUSD per surplus purchase
    type SurplusAuctionThreshold = SurplusAuctionThreshold;  // IF balance threshold
    type SurplusAuctionAmount = SurplusAuctionAmount;        // pUSD per surplus auction
    type ManagerOrigin = EnsureRoot;           // Circuit breaker control
    type MaxOnIdleItems = MaxOnIdleItems;      // Safety limit for on_idle
    type WeightInfo = weights::SubstrateWeight<Runtime>;
}
```

### Suggested Auction Configuration (Per Type)

| Parameter          | Description                  | Liquidation               | Surplus                   |
| ------------------ | ---------------------------- | ------------------------- | ------------------------- |
| `buffer`           | Initial price multiplier     | 1.2 (120%)                | 1.2                       |
| `maximum_duration` | Blocks before restart needed | 3600                      | 3600                      |
| `minimum_price`    | Price floor ratio            | 0.65                      | 0.80                      |
| `chip`             | Percentage keeper incentive  | 0.1%                      | 0%                        |
| `tip`              | Flat keeper incentive        | 1 pUSD                    | 0                         |
| `curve`            | Price decay curve            | SlowedExponentialDecrease | SlowedExponentialDecrease |

## Events

- `AuctionStarted { auction_type, id, tab, lot, owner, starting_block, starting_price, keeper }`
- `Take { auction_type, id, max, price, payment, received, recipient }`
- `AuctionCompleted { auction_type, id, remaining, shortfall }`
- `AuctionRestarted { auction_type, id, starting_price, tab, lot, owner, keeper, incentive }`
- `ConfigUpdated { auction_type, parameter }`
- `StoppedUpdated { level }`
- `SurplusModeUpdated { mode }`
- `SurplusTransferred { amount }`

## Errors

- `AuctionNotFound`: Auction ID doesn't exist
- `AuctionNeedsRestart`: Auction is stale, needs restart first
- `PriceTooHigh`: Current price exceeds buyer's maximum
- `AuctionsStopped`: Circuit breaker blocks new auctions
- `RestartStopped`: Circuit breaker blocks restarts
- `TakeStopped`: Circuit breaker blocks all operations
- `DoesNotNeedRestart`: Auction is still fresh
- `DustyAuction`: Purchase would leave too-small remainder
- `PurchaseTooSmall`: Below minimum purchase amount
- `PriceNotAvailable`: Oracle returned no price
- `InsufficientSurplus`: IF balance below threshold
- `InvalidAuctionType`: Wrong operation for auction type
- `SurplusAuctionAlreadyActive`: Only one surplus auction at a time
- `SurplusAuctionsDisabled`: Mode is DirectTransfer
- `DirectTransferDisabled`: Mode is Auction

## Integration with Vaults

The pallet implements `AuctionsHandler` for vaults to start liquidation auctions:

```rust
pub trait AuctionsHandler<AccountId, Balance> {
    fn start_auction(
        vault_owner: AccountId,
        collateral_amount: Balance,
        debt: DebtComponents<Balance>,
        keeper: AccountId,
    ) -> Result<u32, DispatchError>;
}
```

During takes, it calls back to vaults via `CollateralManager` to:
- Execute collateral transfers
- Burn pUSD (principal repayment)
- Transfer interest/penalty to Insurance Fund
- Complete auctions and handle excess collateral

## On-Idle Housekeeping

The pallet uses `on_idle` to automatically restart stale liquidation auctions:
- Uses cursor-based pagination across blocks
- Respects circuit breaker (blocked at level >= `NoNewAuctionsOrRestarts`)
- Surplus auctions simply end when stale (no restart)
- Limited by `MaxOnIdleItems` per block

## Testing

Run tests with:
```bash
SKIP_WASM_BUILD=1 cargo test -p pallet-auctions
```