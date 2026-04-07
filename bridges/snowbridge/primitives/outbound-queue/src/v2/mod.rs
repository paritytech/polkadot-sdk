// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub mod converter;
pub mod delivery_receipt;
pub mod exporter;
pub mod message;
pub mod simulation;

pub use converter::*;
pub use delivery_receipt::*;
pub use exporter::*;
pub use message::*;
pub use simulation::{
	EthereumExecutionFreeTrader, EthereumExportSimulationBarrier, EthereumSimulationAssetTransactor,
};

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::prelude::*;
use Debug;

/// The `XCM::Transact` payload for calling arbitrary smart contracts on Ethereum.
/// On Ethereum, this call will be dispatched by the agent contract acting as a proxy
/// for the XCM origin.
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub enum ContractCall {
	V1 {
		/// Target contract address
		target: [u8; 20],
		/// ABI-encoded calldata (if non-empty, must be at least 4 bytes — function selector; empty
		/// is allowed for some agent/value flows). Validated in
		/// [`converter::XcmConverter::convert`].
		calldata: Vec<u8>,
		/// Include ether held by the agent contract
		value: u128,
		/// Maximum gas to forward to target contract (must be at least `21_000` — Ethereum
		/// intrinsic tx gas; validated in [`converter::XcmConverter::convert`]).
		gas: u64,
	},
}
