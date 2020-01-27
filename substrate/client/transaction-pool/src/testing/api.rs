// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Test ChainApi

use crate::*;
use codec::Encode;
use parking_lot::RwLock;
use sp_runtime::{
	generic::{self, BlockId},
	traits::{BlakeTwo256, Hash as HashT},
	transaction_validity::{TransactionValidity, ValidTransaction, TransactionValidityError, InvalidTransaction},
};
use std::collections::HashSet;
use substrate_test_runtime_client::{
	runtime::{Index, AccountId, Block, BlockNumber, Extrinsic, Hash, Header, Transfer},
	AccountKeyring::{self, *},
};

#[derive(Default)]
struct ChainState {
	pub block_by_number: HashMap<BlockNumber, Vec<Extrinsic>>,
	pub block_by_hash: HashMap<Hash, Vec<Extrinsic>>,
	pub header_by_number: HashMap<BlockNumber, Header>,
	pub nonces: HashMap<AccountId, u64>,
	pub invalid_hashes: HashSet<Hash>,
}

/// Test Api for transaction pool.
pub struct TestApi {
	valid_modifier: RwLock<Box<dyn Fn(&mut ValidTransaction) + Send + Sync>>,
	chain: RwLock<ChainState>,
	validation_requests: RwLock<Vec<Extrinsic>>,
}

impl TestApi {

	/// Test Api with Alice nonce set initially.
	pub fn with_alice_nonce(nonce: u64) -> Self {
		let api = TestApi {
			valid_modifier: RwLock::new(Box::new(|_| {})),
			chain: Default::default(),
			validation_requests: RwLock::new(Default::default()),
		};

		api.chain.write().nonces.insert(Alice.into(), nonce);

		api
	}

	/// Default Test Api
	pub fn empty() -> Self {
		let api = TestApi {
			valid_modifier: RwLock::new(Box::new(|_| {})),
			chain: Default::default(),
			validation_requests: RwLock::new(Default::default()),
		};

		api
	}

	/// Set hook on modify valid result of transaction.
	pub fn set_valid_modifier(&self, modifier: Box<dyn Fn(&mut ValidTransaction) + Send + Sync>) {
		*self.valid_modifier.write() = modifier;
	}

	/// Push block as a part of canonical chain under given number.
	pub fn push_block(&self, block_number: BlockNumber, xts: Vec<Extrinsic>) {
		let mut chain = self.chain.write();
		chain.block_by_number.insert(block_number, xts);
		chain.header_by_number.insert(block_number, Header {
			number: block_number,
			digest: Default::default(),
			extrinsics_root:  Default::default(),
			parent_hash: Default::default(),
			state_root: Default::default(),
		});
	}

	/// Push a block without a number.
	///
	/// As a part of non-canonical chain.
	pub fn push_fork_block(&self, block_hash: Hash, xts: Vec<Extrinsic>) {
		let mut chain = self.chain.write();
		chain.block_by_hash.insert(block_hash, xts);
	}

	fn hash_and_length_inner(ex: &Extrinsic) -> (Hash, usize) {
		let encoded = ex.encode();
		(BlakeTwo256::hash(&encoded), encoded.len())
	}

	/// Mark some transaction is invalid.
	///
	/// Next time transaction pool will try to validate this
	/// extrinsic, api will return invalid result.
	pub fn add_invalid(&self, xts: &Extrinsic) {
		self.chain.write().invalid_hashes.insert(
			Self::hash_and_length_inner(xts).0
		);
	}

	/// Query validation requests received.
	pub fn validation_requests(&self) -> Vec<Extrinsic> {
		self.validation_requests.read().clone()
	}

	/// Increment nonce in the inner state.
	pub fn increment_nonce(&self, account: AccountId) {
		let mut chain = self.chain.write();
		chain.nonces.entry(account).and_modify(|n| *n += 1).or_insert(1);
	}
}

impl sc_transaction_graph::ChainApi for TestApi {
	type Block = Block;
	type Hash = Hash;
	type Error = error::Error;
	type ValidationFuture = futures::future::Ready<error::Result<TransactionValidity>>;
	type BodyFuture = futures::future::Ready<error::Result<Option<Vec<Extrinsic>>>>;

	fn validate_transaction(
		&self,
		_at: &BlockId<Self::Block>,
		uxt: sc_transaction_graph::ExtrinsicFor<Self>,
	) -> Self::ValidationFuture {
		self.validation_requests.write().push(uxt.clone());

		let chain_nonce = self.chain.read().nonces.get(&uxt.transfer().from).cloned().unwrap_or(0);
		let requires = if chain_nonce == uxt.transfer().nonce {
			vec![]
		} else {
			vec![vec![chain_nonce as u8]]
		};
		let provides = vec![vec![uxt.transfer().nonce as u8]];

		if self.chain.read().invalid_hashes.contains(&self.hash_and_length(&uxt).0) {
			return futures::future::ready(Ok(
				Err(TransactionValidityError::Invalid(InvalidTransaction::Custom(0)))
			))
		}

		let mut validity = ValidTransaction {
			priority: 1,
			requires,
			provides,
			longevity: 64,
			propagate: true,
		};

		(self.valid_modifier.read())(&mut validity);

		futures::future::ready(Ok(Ok(validity)))
	}

	fn block_id_to_number(
		&self,
		at: &BlockId<Self::Block>,
	) -> error::Result<Option<sc_transaction_graph::NumberFor<Self>>> {
		Ok(Some(number_of(at)))
	}

	fn block_id_to_hash(
		&self,
		at: &BlockId<Self::Block>,
	) -> error::Result<Option<sc_transaction_graph::BlockHash<Self>>> {
		Ok(match at {
			generic::BlockId::Hash(x) => Some(x.clone()),
			_ => Some(Default::default()),
		})
	}

	fn hash_and_length(
		&self,
		ex: &sc_transaction_graph::ExtrinsicFor<Self>,
	) -> (Self::Hash, usize) {
		Self::hash_and_length_inner(ex)
	}

	fn block_body(&self, id: &BlockId<Self::Block>) -> Self::BodyFuture {
		futures::future::ready(Ok(if let BlockId::Number(num) = id {
			self.chain.read().block_by_number.get(num).cloned()
		} else {
			None
		}))
	}
}

fn number_of(at: &BlockId<Block>) -> u64 {
	match at {
		generic::BlockId::Number(n) => *n as u64,
		_ => 0,
	}
}

/// Generate transfer extrinsic with a given nonce.
///
/// Part of the test api.
pub fn uxt(who: AccountKeyring, nonce: Index) -> Extrinsic {
	let transfer = Transfer {
		from: who.into(),
		to: AccountId::default(),
		nonce,
		amount: 1,
	};
	let signature = transfer.using_encoded(|e| who.sign(e));
	Extrinsic::Transfer(transfer, signature.into())
}

