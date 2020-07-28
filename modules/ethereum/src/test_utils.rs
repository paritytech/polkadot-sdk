// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Utilities for testing and benchmarking the Ethereum Bridge Pallet.
//!
//! Although the name implies that it is used by tests, it shouldn't be be used _directly_ by tests.
//! Instead these utilities should be used by the Mock runtime, which in turn is used by tests.
//!
//! On the other hand, they may be used directly by the bechmarking module.

// Since this is test code it's fine that not everything is used
#![allow(dead_code)]

use crate::finality::FinalityVotes;
use crate::validators::CHANGE_EVENT_HASH;
use crate::verification::calculate_score;
use crate::{HeaderToImport, Storage, Trait};

use primitives::{
	rlp_encode,
	signatures::{secret_to_address, sign, SignHeader},
	Address, Bloom, Header, Receipt, SealedEmptyStep, H256, U256,
};
use secp256k1::SecretKey;
use sp_std::prelude::*;

/// Gas limit valid in test environment.
pub const GAS_LIMIT: u64 = 0x2000;

/// Test header builder.
pub struct HeaderBuilder {
	header: Header,
	parent_header: Header,
}

impl HeaderBuilder {
	/// Creates default genesis header.
	pub fn genesis() -> Self {
		let current_step = 0u64;
		Self {
			header: Header {
				gas_limit: GAS_LIMIT.into(),
				seal: vec![primitives::rlp_encode(&current_step), vec![]],
				..Default::default()
			},
			parent_header: Default::default(),
		}
	}

	/// Creates default header on top of test parent with given hash.
	#[cfg(test)]
	pub fn with_parent_hash(parent_hash: H256) -> Self {
		Self::with_parent_hash_on_runtime::<crate::mock::TestRuntime, crate::DefaultInstance>(parent_hash)
	}

	/// Creates default header on top of test parent with given number. First parent is selected.
	#[cfg(test)]
	pub fn with_parent_number(parent_number: u64) -> Self {
		Self::with_parent_number_on_runtime::<crate::mock::TestRuntime, crate::DefaultInstance>(parent_number)
	}

	/// Creates default header on top of parent with given hash.
	pub fn with_parent_hash_on_runtime<T: Trait<I>, I: crate::Instance>(parent_hash: H256) -> Self {
		use crate::Headers;
		use frame_support::StorageMap;

		let parent_header = Headers::<T, I>::get(&parent_hash).unwrap().header;
		Self::with_parent(&parent_header)
	}

	/// Creates default header on top of parent with given number. First parent is selected.
	pub fn with_parent_number_on_runtime<T: Trait<I>, I: crate::Instance>(parent_number: u64) -> Self {
		use crate::HeadersByNumber;
		use frame_support::StorageMap;

		let parent_hash = HeadersByNumber::<I>::get(parent_number).unwrap()[0];
		Self::with_parent_hash_on_runtime::<T, I>(parent_hash)
	}

	/// Creates default header on top of non-existent parent.
	#[cfg(test)]
	pub fn with_number(number: u64) -> Self {
		Self::with_parent(&Header {
			number: number - 1,
			seal: vec![primitives::rlp_encode(&(number - 1)), vec![]],
			..Default::default()
		})
	}

	/// Creates default header on top of given parent.
	pub fn with_parent(parent_header: &Header) -> Self {
		let parent_step = parent_header.step().unwrap();
		let current_step = parent_step + 1;
		Self {
			header: Header {
				parent_hash: parent_header.compute_hash(),
				number: parent_header.number + 1,
				gas_limit: GAS_LIMIT.into(),
				seal: vec![primitives::rlp_encode(&current_step), vec![]],
				difficulty: calculate_score(parent_step, current_step, 0),
				..Default::default()
			},
			parent_header: parent_header.clone(),
		}
	}

	/// Update step of this header.
	pub fn step(mut self, step: u64) -> Self {
		let parent_step = self.parent_header.step();
		self.header.seal[0] = rlp_encode(&step);
		self.header.difficulty = parent_step
			.map(|parent_step| calculate_score(parent_step, step, 0))
			.unwrap_or_default();
		self
	}

	/// Adds empty steps to this header.
	pub fn empty_steps(mut self, empty_steps: &[(&SecretKey, u64)]) -> Self {
		let sealed_empty_steps = empty_steps
			.iter()
			.map(|(author, step)| {
				let mut empty_step = SealedEmptyStep {
					step: *step,
					signature: Default::default(),
				};
				let message = empty_step.message(&self.header.parent_hash);
				let signature: [u8; 65] = sign(author, message).into();
				empty_step.signature = signature.into();
				empty_step
			})
			.collect::<Vec<_>>();

		// by default in test configuration headers are generated without empty steps seal
		if self.header.seal.len() < 3 {
			self.header.seal.push(Vec::new());
		}

		self.header.seal[2] = SealedEmptyStep::rlp_of(&sealed_empty_steps);
		self
	}

	/// Update difficulty field of this header.
	pub fn difficulty(mut self, difficulty: U256) -> Self {
		self.header.difficulty = difficulty;
		self
	}

	/// Update extra data field of this header.
	pub fn extra_data(mut self, extra_data: Vec<u8>) -> Self {
		self.header.extra_data = extra_data;
		self
	}

	/// Update gas limit field of this header.
	pub fn gas_limit(mut self, gas_limit: U256) -> Self {
		self.header.gas_limit = gas_limit;
		self
	}

	/// Update gas used field of this header.
	pub fn gas_used(mut self, gas_used: U256) -> Self {
		self.header.gas_used = gas_used;
		self
	}

	/// Update log bloom field of this header.
	pub fn log_bloom(mut self, log_bloom: Bloom) -> Self {
		self.header.log_bloom = log_bloom;
		self
	}

	/// Update receipts root field of this header.
	pub fn receipts_root(mut self, receipts_root: H256) -> Self {
		self.header.receipts_root = receipts_root;
		self
	}

	/// Update timestamp field of this header.
	pub fn timestamp(mut self, timestamp: u64) -> Self {
		self.header.timestamp = timestamp;
		self
	}

	/// Update transactions root field of this header.
	pub fn transactions_root(mut self, transactions_root: H256) -> Self {
		self.header.transactions_root = transactions_root;
		self
	}

	/// Signs header by given author.
	pub fn sign_by(self, author: &SecretKey) -> Header {
		self.header.sign_by(author)
	}

	/// Signs header by given authors set.
	pub fn sign_by_set(self, authors: &[SecretKey]) -> Header {
		self.header.sign_by_set(authors)
	}
}

/// Helper function for getting a genesis header which has been signed by an authority.
pub fn build_genesis_header(author: &SecretKey) -> Header {
	let genesis = HeaderBuilder::genesis();
	genesis.header.sign_by(&author)
}

/// Helper function for building a custom child header which has been signed by an authority.
pub fn build_custom_header<F>(author: &SecretKey, previous: &Header, customize_header: F) -> Header
where
	F: FnOnce(Header) -> Header,
{
	let new_header = HeaderBuilder::with_parent(&previous);
	let custom_header = customize_header(new_header.header);
	custom_header.sign_by(author)
}

/// Insert unverified header into storage.
pub fn insert_header<S: Storage>(storage: &mut S, header: Header) {
	storage.insert_header(HeaderToImport {
		context: storage.import_context(None, &header.parent_hash).unwrap(),
		is_best: true,
		id: header.compute_id(),
		header,
		total_difficulty: 0.into(),
		enacted_change: None,
		scheduled_change: None,
		finality_votes: FinalityVotes::default(),
	});
}

pub fn validators_change_receipt(parent_hash: H256) -> Receipt {
	use primitives::{LogEntry, TransactionOutcome};

	Receipt {
		gas_used: 0.into(),
		log_bloom: (&[0xff; 256]).into(),
		outcome: TransactionOutcome::Unknown,
		logs: vec![LogEntry {
			address: [3; 20].into(),
			topics: vec![CHANGE_EVENT_HASH.into(), parent_hash],
			data: vec![
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 7, 7, 7, 7,
				7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
			],
		}],
	}
}

pub mod validator_utils {
	use super::*;

	/// Return key pair of given test validator.
	pub fn validator(index: usize) -> SecretKey {
		let mut raw_secret = [0u8; 32];
		raw_secret[..8].copy_from_slice(&(index + 1).to_le_bytes());
		SecretKey::parse(&raw_secret).unwrap()
	}

	/// Return key pairs of all test validators.
	pub fn validators(count: usize) -> Vec<SecretKey> {
		(0..count).map(validator).collect()
	}

	/// Return address of test validator.
	pub fn validator_address(index: usize) -> Address {
		secret_to_address(&validator(index))
	}

	/// Return addresses of all test validators.
	pub fn validators_addresses(count: usize) -> Vec<Address> {
		(0..count).map(validator_address).collect()
	}
}
