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

use sp_blockchain::Error as ClientError;
use sp_consensus::block_validation::{BlockAnnounceValidator, Validation};
use sp_runtime::traits::Block as BlockT;

use polkadot_network::legacy::gossip::{GossipMessage, GossipStatement};
use polkadot_primitives::parachain::ValidatorId;
use polkadot_statement_table::{SignedStatement, Statement};
use polkadot_validation::check_statement;

use codec::{Decode, Encode};

use std::marker::PhantomData;

/// Validate that data is a valid justification from a relay-chain validator that the block is a
/// valid parachain-block candidate.
/// Data encoding is just `GossipMessage`, the relay-chain validator candidate statement message is
/// the justification.
///
/// Note: if no justification is provided the annouce is considered valid.
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
	fn validate(
		&mut self,
		header: &B::Header,
		mut data: &[u8],
	) -> Result<Validation, Box<dyn std::error::Error + Send>> {
		// If no data is provided the announce is valid.
		if data.is_empty() {
			return Ok(Validation::Success);
		}

		// Check data is a gossip message.
		let gossip_message = GossipMessage::decode(&mut data).map_err(|_| {
			Box::new(ClientError::BadJustification(
				"cannot decode block announced justification, must be a gossip message".to_string(),
			)) as Box<_>
		})?;

		// Check message is a gossip statement.
		let gossip_statement = match gossip_message {
			GossipMessage::Statement(gossip_statement) => gossip_statement,
			_ => {
				return Err(Box::new(ClientError::BadJustification(
					"block announced justification statement must be a gossip statement"
						.to_string(),
				)) as Box<_>)
			}
		};

		let GossipStatement {
			relay_chain_leaf,
			signed_statement: SignedStatement {
				statement,
				signature,
				sender,
			},
		} = gossip_statement;

		// Check that the signer is a legit validator.
		let signer = self.authorities.get(sender as usize).ok_or_else(|| {
			Box::new(ClientError::BadJustification(
				"block accounced justification signer is a validator index out of bound"
					.to_string(),
			)) as Box<_>
		})?;

		// Check statement is correctly signed.
		if !check_statement(&statement, &signature, signer.clone(), &relay_chain_leaf) {
			return Err(Box::new(ClientError::BadJustification(
				"block announced justification signature is invalid".to_string(),
			)) as Box<_>);
		}

		// Check statement is a candidate statement.
		let candidate_receipt = match statement {
			Statement::Candidate(candidate_receipt) => candidate_receipt,
			_ => {
				return Err(Box::new(ClientError::BadJustification(
					"block announced justification statement must be a candidate statement"
						.to_string(),
				)) as Box<_>)
			}
		};

		// Check the header in the candidate_receipt match header given header.
		if header.encode() != candidate_receipt.head_data.0 {
			return Err(Box::new(ClientError::BadJustification(
				"block announced header does not match the one justified".to_string(),
			)) as Box<_>);
		}

		Ok(Validation::Success)
	}
}
