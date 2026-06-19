// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub mod converter;
pub mod delivery_receipt;
pub mod exporter;
pub mod message;

pub use converter::*;
pub use delivery_receipt::*;
pub use message::*;

use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_std::prelude::*;
use Debug;

/// The `XCM::Transact` payload for calling arbitrary smart contracts on Ethereum.
/// On Ethereum, this call will be dispatched by the agent contract acting as a proxy
/// for the XCM origin.
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub enum ContractCall {
	/// Call a single contract.
	Single {
		/// Target contract address
		target: [u8; 20],
		/// ABI-encoded calldata
		calldata: Vec<u8>,
		/// Include ether held by the agent contract
		value: u128,
		/// Maximum gas to forward to target contract
		gas: u64,
	},
	/// Call multiple contracts atomically. The sub-calls are executed in order and the whole
	/// command reverts on the first failure.
	Multi {
		/// Sub-calls to execute, in order
		calls: Vec<ContractCallEntry>,
		/// Maximum gas to forward to the target contracts
		gas: u64,
	},
}

/// A single sub-call within a [`ContractCall::Multi`] batch.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Debug, TypeInfo)]
pub struct ContractCallEntry {
	/// Target contract address
	pub target: [u8; 20],
	/// ABI-encoded calldata
	pub calldata: Vec<u8>,
	/// Include ether held by the agent contract
	pub value: u128,
}
