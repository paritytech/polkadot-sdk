// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Types for representing inbound messages

use codec::{Decode, Encode};
use frame_support::PalletError;
use scale_info::TypeInfo;
use sp_core::{H160, H256};
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

/// A trait for verifying inbound messages from Ethereum.
pub trait Verifier {
	fn verify(event: &Log, proof: &Proof) -> Result<(), VerificationError>;
}

#[derive(Clone, Encode, Decode, RuntimeDebug, PalletError, TypeInfo)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub enum VerificationError {
	/// Execution header is missing
	HeaderNotFound,
	/// Event log was not found in the verified transaction receipt
	LogNotFound,
	/// Event log has an invalid format
	InvalidLog,
	/// Unable to verify the transaction receipt with the provided proof
	InvalidProof,
}

pub type MessageNonce = u64;

/// A bridge message from the Gateway contract on Ethereum
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct Message {
	/// Event log emitted by Gateway contract
	pub event_log: Log,
	/// Inclusion proof for a transaction receipt containing the event log
	pub proof: Proof,
}

const MAX_TOPICS: usize = 4;

#[derive(Clone, RuntimeDebug)]
pub enum LogValidationError {
	TooManyTopics,
}

/// Event log
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct Log {
	pub address: H160,
	pub topics: Vec<H256>,
	pub data: Vec<u8>,
}

impl Log {
	pub fn validate(&self) -> Result<(), LogValidationError> {
		if self.topics.len() > MAX_TOPICS {
			return Err(LogValidationError::TooManyTopics)
		}
		Ok(())
	}
}

/// Inclusion proof for a transaction receipt
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct Proof {
	// The block hash of the block in which the receipt was included.
	pub block_hash: H256,
	// The index of the transaction (and receipt) within the block.
	pub tx_index: u32,
	// Proof keys and values (receipts tree)
	pub data: (Vec<Vec<u8>>, Vec<Vec<u8>>),
}
