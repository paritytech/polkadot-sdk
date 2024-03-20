// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use snowbridge_core::inbound::{
	VerificationError::{self, *},
	*,
};
use snowbridge_ethereum::Receipt;

impl<T: Config> Verifier for Pallet<T> {
	/// Verify a message by verifying the existence of the corresponding
	/// Ethereum log in a block. Returns the log if successful. The execution header containing
	/// the log should be in the beacon client storage, meaning it has been verified and is an
	/// ancestor of a finalized beacon block.
	fn verify(event_log: &Log, proof: &Proof) -> Result<(), VerificationError> {
		log::info!(
			target: "ethereum-client",
			"💫 Verifying message with block hash {}",
			proof.block_hash,
		);

		let header = <ExecutionHeaderBuffer<T>>::get(proof.block_hash).ok_or(HeaderNotFound)?;

		let receipt = match Self::verify_receipt_inclusion(header.receipts_root, proof) {
			Ok(receipt) => receipt,
			Err(err) => {
				log::error!(
					target: "ethereum-client",
					"💫 Verification of receipt inclusion failed for block {}: {:?}",
					proof.block_hash,
					err
				);
				return Err(err)
			},
		};

		log::trace!(
			target: "ethereum-client",
			"💫 Verified receipt inclusion for transaction at index {} in block {}",
			proof.tx_index, proof.block_hash,
		);

		event_log.validate().map_err(|_| InvalidLog)?;

		// Convert snowbridge_core::inbound::Log to snowbridge_ethereum::Log.
		let event_log = snowbridge_ethereum::Log {
			address: event_log.address,
			topics: event_log.topics.clone(),
			data: event_log.data.clone(),
		};

		if !receipt.contains_log(&event_log) {
			log::error!(
				target: "ethereum-client",
				"💫 Event log not found in receipt for transaction at index {} in block {}",
				proof.tx_index, proof.block_hash,
			);
			return Err(LogNotFound)
		}

		log::info!(
			target: "ethereum-client",
			"💫 Receipt verification successful for {}",
			proof.block_hash,
		);

		Ok(())
	}
}

impl<T: Config> Pallet<T> {
	/// Verifies that the receipt encoded in `proof.data` is included in the block given by
	/// `proof.block_hash`.
	pub fn verify_receipt_inclusion(
		receipts_root: H256,
		proof: &Proof,
	) -> Result<Receipt, VerificationError> {
		let result = verify_receipt_proof(receipts_root, &proof.data.1).ok_or(InvalidProof)?;

		match result {
			Ok(receipt) => Ok(receipt),
			Err(err) => {
				log::trace!(
					target: "ethereum-client",
					"💫 Failed to decode transaction receipt: {}",
					err
				);
				Err(InvalidProof)
			},
		}
	}
}
