// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Gas ceilings for dispatching a command on the Gateway contract, shared by the v1 and v2
//! `ConstantGasMeter`.
//!
//! Each ceiling is at least 1.5x the handler's gas on the production build under `forge test
//! --hardfork amsterdam` (EIP-8037 state creation, EIP-8038 state access).
/// Halting writes the `mode` slot from zero. Measured 116_678.
pub const SET_OPERATING_MODE: u64 = 200_000;

/// Proxy update; `maximum_required_gas` is added on top. Measured 24_550.
pub const UPGRADE_BASE: u64 = 75_000;

/// Ether to a new account, or an ERC20 transfer to a fresh recipient. Measured 211_050.
pub const UNLOCK_NATIVE_TOKEN: u64 = 600_000;

/// Deploys a Token. Measured 5_396_266.
pub const REGISTER_FOREIGN_TOKEN: u64 = 8_500_000;

/// A first mint allocates two slots. Measured 237_657.
pub const MINT_FOREIGN_TOKEN: u64 = 400_000;

/// v1 only. The same agent transfer as [`UNLOCK_NATIVE_TOKEN`]. Measured 212_367.
pub const TRANSFER_TOKEN: u64 = UNLOCK_NATIVE_TOKEN;

/// v1 only. Measured 17_637.
pub const SET_TOKEN_TRANSFER_FEES: u64 = 90_000;

/// v1 only. Measured 42_528.
pub const SET_PRICING_PARAMETERS: u64 = 90_000;
