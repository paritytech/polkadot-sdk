# PSM Primitives

Shared primitives for the pUSD pallets.

## Overview

This crate provides common types and traits used by the `pallet-vaults` and `pallet-auctions`
pallets to enable a decoupled, modular CDP (Collateralized Debt Position) system.

The design follows a separation of concerns:
- **Vaults pallet**: Manages collateral, debt, minting/burning, and asset operations
- **Auctions pallet**: Manages auction state, price decay, and purchase logic
- **PSM Primitives**: Defines the interface between them

## Types

### `PaymentBreakdown<Balance>`

Describes how a pUSD payment is distributed during an auction purchase.

```rust
pub struct PaymentBreakdown<Balance> {
    /// Amount to burn (principal debt repayment)
    pub burn: Balance,
    /// Amount to transfer to keeper (incentive)
    pub keeper: Balance,
    /// Amount to transfer to Insurance Fund (interest + penalty)
    pub insurance_fund: Balance,
}
```

### `PurchaseParams<AccountId, Balance>`

Parameters for executing a collateral purchase during auction `take()`.

```rust
pub struct PurchaseParams<AccountId, Balance> {
    /// Original vault owner (collateral released from their seized hold)
    pub vault_owner: AccountId,
    /// Account paying pUSD for the collateral
    pub buyer: AccountId,
    /// Account receiving the collateral (may differ from buyer)
    pub recipient: AccountId,
    /// Account receiving the keeper incentive
    pub keeper: AccountId,
    /// Amount of collateral to transfer
    pub collateral_amount: Balance,
    /// How the pUSD payment is distributed
    pub payment: PaymentBreakdown<Balance>,
}
```

## Traits

### `AuctionsHandler`

Implemented by `pallet-auctions`, called by `pallet-vaults` to start liquidation auctions.

```rust
pub trait AuctionsHandler<AccountId, Balance> {
    /// Start a new auction for liquidating vault collateral.
    fn start_auction(
        vault_owner: &AccountId,
        collateral_amount: Balance,
        principal: Balance,
        accrued_interest: Balance,
        penalty: Balance,
        keeper: &AccountId,
    ) -> Result<u32, DispatchError>;
}
```

### `CollateralManager`

Implemented by `pallet-vaults`, called by `pallet-auctions` for asset operations.

```rust
pub trait CollateralManager<AccountId> {
    type Balance: Balance + FixedPointOperand;

    /// Get current collateral price from oracle.
    fn get_dot_price() -> Option<FixedU128>;

    /// Execute a purchase: collect pUSD, transfer collateral.
    fn execute_purchase(params: PurchaseParams<AccountId, Self::Balance>) -> DispatchResult;

    /// Complete auction: return excess collateral, record shortfall.
    fn complete_auction(
        vault_owner: &AccountId,
        remaining_collateral: Self::Balance,
        shortfall: Self::Balance,
    ) -> DispatchResult;
}
```

## License

Apache-2.0
