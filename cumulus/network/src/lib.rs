// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Cumulus-specific network implementation.
//!
//! Contains message send between collators and logic to process them.

use substrate_client::error::{Error as ClientError};
use sr_primitives::traits::{Block as BlockT};
use substrate_consensus_common::block_validation::{Validation, BlockAnnounceValidator};

use polkadot_primitives::{
	Hash as PHash, parachain::{CandidateReceipt, ValidatorIndex, ValidatorSignature, ValidatorId},
};
use polkadot_statement_table::Statement;
use polkadot_validation::check_statement;

use codec::{Decode, Encode};

use std::marker::PhantomData;

/// Justification that a parachain block is the parachain block candidate of one of the relay chain
/// validator.
#[derive(Encode, Decode)]
pub struct BlockCandidateJustification {
	/// Receipt of the parachain block candidate of the signer.
	candidate_receipt: CandidateReceipt,
	/// Signer of `signature`.
	signer: ValidatorIndex,
	/// Signature of the Candidate statement with `candidate_receipt`.
	signature: ValidatorSignature,
	/// The parent block of which the candidate must be include.
	relay_chain_parent_hash: PHash,
}

/// Validate that data is a valid justification form a relay-chain validator that the block is a
/// valid parachain-block candidate.
pub struct JustifiedBlockAnnounceValidator<B> {
	authorities: Vec<ValidatorId>,
	phantom: PhantomData<B>,
}

impl<B: BlockT> JustifiedBlockAnnounceValidator<B> {
	pub fn new(authorities: Vec<ValidatorId>) -> Self {
		Self {
			authorities,
			phantom: Default::default(),
		}
	}
}

impl<B: BlockT> BlockAnnounceValidator<B> for JustifiedBlockAnnounceValidator<B> {
	fn validate(&mut self, header: &B::Header, mut data: &[u8])
		-> Result<Validation, Box<dyn std::error::Error + Send>>
	{
		let justification = BlockCandidateJustification::decode(&mut data)
			.map_err(|_| Box::new(ClientError::BadJustification(
				"cannot decode block candidate justification".to_string()
			)) as Box<_>)?;

		// Check the header in the candidate_receipt match header given header.
		if header.encode() != justification.candidate_receipt.head_data.0 {
			return Err(Box::new(ClientError::BadJustification(
				"block candidate header does not match its justification".to_string()
			)) as Box<_>)
		}

		// Check that the signer is a legit validator.
		let signer = self.authorities.get(justification.signer as usize)
			.ok_or_else(|| Box::new(ClientError::BadJustification(
				"block candidate justification signer is a validator index out of bound".to_string()
			)) as Box<_>)?;

		// Check statement is signed.
		let statement = Statement::Candidate(justification.candidate_receipt);
		if !check_statement(
			&statement,
			&justification.signature,
			signer.clone(),
			&justification.relay_chain_parent_hash
		) {
			return Err(Box::new(ClientError::BadJustification(
				"block candidate justification signature is invalid".to_string()
			)) as Box<_>)
		}

		Ok(Validation::Success)
	}
}
