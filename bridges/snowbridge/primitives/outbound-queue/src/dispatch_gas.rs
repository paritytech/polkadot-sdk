// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Gas ceilings for dispatching a command on the Gateway contract, shared by the v1 and v2
//! `ConstantGasMeter`.
//!
//! Measured on the Gateway handlers with `forge test --hardfork amsterdam --gas-report`
//! (Foundry v1.8.3, revm's Glamsterdam devnet-8 schedule), then buffered for the EIP-150 63/64
//! rule and future EVM upgrades. Glamsterdam prices new state separately (EIP-8037: 183_600 for
//! a new account, 97_920 for a new storage slot) and reprices state access (EIP-8038).
//! Re-check once the EIP-8038 values are final.

/// Halting writes the `mode` slot from zero, allocating a new slot (110_020); measured 116_162.
pub const SET_OPERATING_MODE: u64 = 200_000;

/// Proxy update before the initializer runs; `maximum_required_gas` is added on top.
/// Measured 27_142.
pub const UPGRADE_BASE: u64 = 75_000;

/// Ether to a new account (183_600), or an ERC20 transfer to a fresh recipient (97_920),
/// plus AgentExecutor overhead.
pub const UNLOCK_NATIVE_TOKEN: u64 = 600_000;

/// Deploys a Token: 2336 code bytes (3_574_080) + new account (183_600) + five slots for
/// name, symbol, tokenAddressOf and the two TokenInfo slots (489_600) = 4_247_280, plus
/// roughly 100_000 execution.
pub const REGISTER_FOREIGN_TOKEN: u64 = 7_000_000;

/// A first mint allocates totalSupply and the recipient balance: two slots.
pub const MINT_FOREIGN_TOKEN: u64 = 400_000;

/// v1 only. The same agent transfer as [`UNLOCK_NATIVE_TOKEN`], Ether or ERC20, so the same
/// ceiling.
pub const TRANSFER_TOKEN: u64 = UNLOCK_NATIVE_TOKEN;

/// Writes existing slots only; measured 19_734. v1 only.
pub const SET_TOKEN_TRANSFER_FEES: u64 = 90_000;

/// Writes existing slots only; measured 43_837. v1 only.
pub const SET_PRICING_PARAMETERS: u64 = 90_000;
