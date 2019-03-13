// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! System manager: Handles all of the top-level stuff; executing block/transaction, setting code
//! and depositing logs.

use rstd::prelude::*;
use runtime_io::{storage_root, enumerated_trie_root, storage_changes_root, twox_128};
use runtime_support::storage::{self, StorageValue, StorageMap};
use runtime_support::storage_items;
use runtime_primitives::traits::{Hash as HashT, BlakeTwo256, Digest as DigestT};
use runtime_primitives::generic;
use runtime_primitives::{ApplyError, ApplyOutcome, ApplyResult, transaction_validity::TransactionValidity};
use parity_codec::{KeyedVec, Encode};
use super::{AccountId, BlockNumber, Extrinsic, Transfer, H256 as Hash, Block, Header, Digest};
use primitives::{Blake2Hasher, storage::well_known_keys};
use primitives::ed25519::Public as AuthorityId;

const NONCE_OF: &[u8] = b"nonce:";
const BALANCE_OF: &[u8] = b"balance:";

storage_items! {
	ExtrinsicData: b"sys:xtd" => required map [ u32 => Vec<u8> ];
	// The current block number being processed. Set by `execute_block`.
	Number: b"sys:num" => required BlockNumber;
	ParentHash: b"sys:pha" => required Hash;
	NewAuthorities: b"sys:new_auth" => Vec<AuthorityId>;
}

pub fn balance_of_key(who: AccountId) -> Vec<u8> {
	who.to_keyed_vec(BALANCE_OF)
}

pub fn balance_of(who: AccountId) -> u64 {
	storage::get_or(&balance_of_key(who), 0)
}

pub fn nonce_of(who: AccountId) -> u64 {
	storage::get_or(&who.to_keyed_vec(NONCE_OF), 0)
}

/// Get authorities ar given block.
pub fn authorities() -> Vec<AuthorityId> {
	let len: u32 = storage::unhashed::get(well_known_keys::AUTHORITY_COUNT)
		.expect("There are always authorities in test-runtime");
	(0..len)
		.map(|i| storage::unhashed::get(&i.to_keyed_vec(well_known_keys::AUTHORITY_PREFIX))
			.expect("Authority is properly encoded in test-runtime")
		)
		.collect()
}

pub fn initialise_block(header: &Header) {
	// populate environment.
	<Number>::put(&header.number);
	<ParentHash>::put(&header.parent_hash);
	storage::unhashed::put(well_known_keys::EXTRINSIC_INDEX, &0u32);
}

/// Actually execute all transitioning for `block`.
pub fn polish_block(block: &mut Block) {
	let header = &mut block.header;

	// check transaction trie root represents the transactions.
	let txs = block.extrinsics.iter().map(Encode::encode).collect::<Vec<_>>();
	let txs = txs.iter().map(Vec::as_slice).collect::<Vec<_>>();
	let txs_root = enumerated_trie_root::<Blake2Hasher>(&txs).into();
	info_expect_equal_hash(&txs_root, &header.extrinsics_root);
	header.extrinsics_root = txs_root;

	// execute transactions
	block.extrinsics.iter().enumerate().for_each(|(i, e)| {
		storage::unhashed::put(well_known_keys::EXTRINSIC_INDEX, &(i as u32));
		execute_transaction_backend(e).unwrap_or_else(|_| panic!("Invalid transaction"));
		storage::unhashed::kill(well_known_keys::EXTRINSIC_INDEX);
	});

	header.state_root = storage_root().into();

	// check digest
	let mut digest = Digest::default();
	if let Some(storage_changes_root) = storage_changes_root(header.parent_hash.into(), header.number - 1) {
		digest.push(generic::DigestItem::ChangesTrieRoot(storage_changes_root.into()));
	}
	if let Some(new_authorities) = <NewAuthorities>::take() {
		digest.push(generic::DigestItem::AuthoritiesChange(new_authorities));
	}
	header.digest = digest;
}

pub fn execute_block(block: Block) {
	let ref header = block.header;

	// check transaction trie root represents the transactions.
	let txs = block.extrinsics.iter().map(Encode::encode).collect::<Vec<_>>();
	let txs = txs.iter().map(Vec::as_slice).collect::<Vec<_>>();
	let txs_root = enumerated_trie_root::<Blake2Hasher>(&txs).into();
	info_expect_equal_hash(&txs_root, &header.extrinsics_root);
	assert!(txs_root == header.extrinsics_root, "Transaction trie root must be valid.");

	// execute transactions
	block.extrinsics.iter().enumerate().for_each(|(i, e)| {
	storage::unhashed::put(well_known_keys::EXTRINSIC_INDEX, &(i as u32));
	execute_transaction_backend(e).unwrap_or_else(|_| panic!("Invalid transaction"));
	storage::unhashed::kill(well_known_keys::EXTRINSIC_INDEX);
	});

	// check storage root.
	let storage_root = storage_root().into();
	info_expect_equal_hash(&storage_root, &header.state_root);
	assert!(storage_root == header.state_root, "Storage root must match that calculated.");

	// check digest
	let mut digest = Digest::default();
	if let Some(storage_changes_root) = storage_changes_root(header.parent_hash.into(), header.number - 1) {
		digest.push(generic::DigestItem::ChangesTrieRoot(storage_changes_root.into()));
	}
	if let Some(new_authorities) = <NewAuthorities>::take() {
		digest.push(generic::DigestItem::AuthoritiesChange(new_authorities));
	}
	assert!(digest == header.digest, "Header digest items must match that calculated.");
}

/// Execute a transaction outside of the block execution function.
/// This doesn't attempt to validate anything regarding the block.
pub fn validate_transaction(utx: Extrinsic) -> TransactionValidity {
	if check_signature(&utx).is_err() {
		return TransactionValidity::Invalid(ApplyError::BadSignature as i8);
	}

	let tx = utx.transfer();
	let nonce_key = tx.from.to_keyed_vec(NONCE_OF);
	let expected_nonce: u64 = storage::get_or(&nonce_key, 0);
	if tx.nonce < expected_nonce {
		return TransactionValidity::Invalid(ApplyError::Stale as i8);
	}
	if tx.nonce > expected_nonce + 64 {
		return TransactionValidity::Unknown(ApplyError::Future as i8);
	}

	let hash = |from: &AccountId, nonce: u64| {
		twox_128(&nonce.to_keyed_vec(&from.encode())).to_vec()
	};
	let requires = if tx.nonce != expected_nonce && tx.nonce > 0 {
		let mut deps = Vec::new();
		deps.push(hash(&tx.from, tx.nonce - 1));
		deps
	} else { Vec::new() };

	let provides = {
		let mut p = Vec::new();
		p.push(hash(&tx.from, tx.nonce));
		p
	};

	TransactionValidity::Valid {
		priority: tx.amount,
		requires,
		provides,
		longevity: 64,
	}
}

/// Execute a transaction outside of the block execution function.
/// This doesn't attempt to validate anything regarding the block.
pub fn execute_transaction(utx: Extrinsic) -> ApplyResult {
	let extrinsic_index: u32 = storage::unhashed::get(well_known_keys::EXTRINSIC_INDEX).unwrap();
	let result = execute_transaction_backend(&utx);
	ExtrinsicData::insert(extrinsic_index, utx.encode());
	storage::unhashed::put(well_known_keys::EXTRINSIC_INDEX, &(extrinsic_index + 1));
	result
}

/// Finalise the block.
pub fn finalise_block() -> Header {
	let extrinsic_index: u32 = storage::unhashed::take(well_known_keys::EXTRINSIC_INDEX).unwrap();
	let txs: Vec<_> = (0..extrinsic_index).map(ExtrinsicData::take).collect();
	let txs = txs.iter().map(Vec::as_slice).collect::<Vec<_>>();
	let extrinsics_root = enumerated_trie_root::<Blake2Hasher>(&txs).into();

	let number = <Number>::take();
	let parent_hash = <ParentHash>::take();
	let storage_root = BlakeTwo256::storage_root();
	let storage_changes_root = BlakeTwo256::storage_changes_root(parent_hash, number - 1);

	let mut digest = Digest::default();
	if let Some(storage_changes_root) = storage_changes_root {
		digest.push(generic::DigestItem::ChangesTrieRoot(storage_changes_root));
	}
	if let Some(new_authorities) = <NewAuthorities>::take() {
		digest.push(generic::DigestItem::AuthoritiesChange(new_authorities));
	}

	Header {
		number,
		extrinsics_root,
		state_root: storage_root,
		parent_hash,
		digest: digest,
	}
}

#[inline(always)]
fn check_signature(utx: &Extrinsic) -> Result<(), ApplyError> {
	use runtime_primitives::traits::BlindCheckable;
	utx.clone().check().map_err(|_| ApplyError::BadSignature)?;
	Ok(())
}

fn execute_transaction_backend(utx: &Extrinsic) -> ApplyResult {
	check_signature(utx)?;
	match utx {
		Extrinsic::Transfer(ref transfer, _) => execute_transfer_backend(transfer),
		Extrinsic::AuthoritiesChange(ref new_auth) => execute_new_authorities_backend(new_auth),
	}
}

fn execute_transfer_backend(tx: &Transfer) -> ApplyResult {
	// check nonce
	let nonce_key = tx.from.to_keyed_vec(NONCE_OF);
	let expected_nonce: u64 = storage::get_or(&nonce_key, 0);
	if !(tx.nonce == expected_nonce) {
		return Err(ApplyError::Stale)
	}

	// increment nonce in storage
	storage::put(&nonce_key, &(expected_nonce + 1));

	// check sender balance
	let from_balance_key = tx.from.to_keyed_vec(BALANCE_OF);
	let from_balance: u64 = storage::get_or(&from_balance_key, 0);

	// enact transfer
	if !(tx.amount <= from_balance) {
		return Err(ApplyError::CantPay)
	}
	let to_balance_key = tx.to.to_keyed_vec(BALANCE_OF);
	let to_balance: u64 = storage::get_or(&to_balance_key, 0);
	storage::put(&from_balance_key, &(from_balance - tx.amount));
	storage::put(&to_balance_key, &(to_balance + tx.amount));
	Ok(ApplyOutcome::Success)
}

fn execute_new_authorities_backend(new_authorities: &[AuthorityId]) -> ApplyResult {
	let new_authorities: Vec<AuthorityId> = new_authorities.iter().cloned().collect();
	<NewAuthorities>::put(new_authorities);
	Ok(ApplyOutcome::Success)
}

#[cfg(feature = "std")]
fn info_expect_equal_hash(given: &Hash, expected: &Hash) {
	use primitives::hexdisplay::HexDisplay;
	if given != expected {
		println!(
			"Hash: given={}, expected={}",
			HexDisplay::from(given.as_fixed_bytes()),
			HexDisplay::from(expected.as_fixed_bytes())
		);
	}
}

#[cfg(not(feature = "std"))]
fn info_expect_equal_hash(given: &Hash, expected: &Hash) {
	if given != expected {
		::runtime_io::print("Hash not equal");
		::runtime_io::print(given.as_bytes());
		::runtime_io::print(expected.as_bytes());
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use runtime_io::{with_externalities, twox_128, TestExternalities};
	use parity_codec::{Joiner, KeyedVec};
	use substrate_test_client::{AuthorityKeyring, AccountKeyring};
	use crate::{Header, Extrinsic, Transfer};
	use primitives::{Blake2Hasher, map};
	use primitives::storage::well_known_keys;
	use substrate_executor::WasmExecutor;

	const WASM_CODE: &'static [u8] =
			include_bytes!("../wasm/target/wasm32-unknown-unknown/release/substrate_test_runtime.compact.wasm");

	fn new_test_ext() -> TestExternalities<Blake2Hasher> {
		TestExternalities::new(map![
			twox_128(b"latest").to_vec() => vec![69u8; 32],
			twox_128(well_known_keys::AUTHORITY_COUNT).to_vec() => vec![].and(&3u32),
			twox_128(&0u32.to_keyed_vec(well_known_keys::AUTHORITY_PREFIX)).to_vec() => AuthorityKeyring::Alice.to_raw_public().to_vec(),
			twox_128(&1u32.to_keyed_vec(well_known_keys::AUTHORITY_PREFIX)).to_vec() => AuthorityKeyring::Bob.to_raw_public().to_vec(),
			twox_128(&2u32.to_keyed_vec(well_known_keys::AUTHORITY_PREFIX)).to_vec() => AuthorityKeyring::Charlie.to_raw_public().to_vec(),
			twox_128(&AccountKeyring::Alice.to_raw_public().to_keyed_vec(b"balance:")).to_vec() => vec![111u8, 0, 0, 0, 0, 0, 0, 0]
		])
	}

	fn construct_signed_tx(tx: Transfer) -> Extrinsic {
		let signature = AccountKeyring::from_public(&tx.from).unwrap().sign(&tx.encode()).into();
		Extrinsic::Transfer(tx, signature)
	}

	fn block_import_works<F>(block_executor: F) where F: Fn(Block, &mut TestExternalities<Blake2Hasher>) {
		let h = Header {
			parent_hash: [69u8; 32].into(),
			number: 1,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest: Default::default(),
		};
		let mut b = Block {
			header: h,
			extrinsics: vec![],
		};

		with_externalities(&mut new_test_ext(), || polish_block(&mut b));

		block_executor(b, &mut new_test_ext());
	}

	#[test]
	fn block_import_works_native() {
		block_import_works(|b, ext| {
			with_externalities(ext, || {
				execute_block(b);
			});
		});
	}

	#[test]
	fn block_import_works_wasm() {
		block_import_works(|b, ext| {
			WasmExecutor::new().call(ext, 8, &WASM_CODE, "Core_execute_block", &b.encode()).unwrap();
		})
	}

	fn block_import_with_transaction_works<F>(block_executor: F) where F: Fn(Block, &mut TestExternalities<Blake2Hasher>) {
		let mut b1 = Block {
			header: Header {
				parent_hash: [69u8; 32].into(),
				number: 1,
				state_root: Default::default(),
				extrinsics_root: Default::default(),
				digest: Default::default(),
			},
			extrinsics: vec![
				construct_signed_tx(Transfer {
					from: AccountKeyring::Alice.into(),
					to: AccountKeyring::Bob.into(),
					amount: 69,
					nonce: 0,
				})
			],
		};

		let mut dummy_ext = new_test_ext();
		with_externalities(&mut dummy_ext, || polish_block(&mut b1));

		let mut b2 = Block {
			header: Header {
				parent_hash: b1.header.hash(),
				number: 2,
				state_root: Default::default(),
				extrinsics_root: Default::default(),
				digest: Default::default(),
			},
			extrinsics: vec![
				construct_signed_tx(Transfer {
					from: AccountKeyring::Bob.into(),
					to: AccountKeyring::Alice.into(),
					amount: 27,
					nonce: 0,
				}),
				construct_signed_tx(Transfer {
					from: AccountKeyring::Alice.into(),
					to: AccountKeyring::Charlie.into(),
					amount: 69,
					nonce: 1,
				}),
			],
		};

		with_externalities(&mut dummy_ext, || polish_block(&mut b2));
		drop(dummy_ext);

		let mut t = new_test_ext();

		with_externalities(&mut t, || {
			assert_eq!(balance_of(AccountKeyring::Alice.into()), 111);
			assert_eq!(balance_of(AccountKeyring::Bob.into()), 0);
		});

		block_executor(b1, &mut t);

		with_externalities(&mut t, || {
			assert_eq!(balance_of(AccountKeyring::Alice.into()), 42);
			assert_eq!(balance_of(AccountKeyring::Bob.into()), 69);
		});

		block_executor(b2, &mut t);

		with_externalities(&mut t, || {
			assert_eq!(balance_of(AccountKeyring::Alice.into()), 0);
			assert_eq!(balance_of(AccountKeyring::Bob.into()), 42);
			assert_eq!(balance_of(AccountKeyring::Charlie.into()), 69);
		});
	}

	#[test]
	fn block_import_with_transaction_works_native() {
		block_import_with_transaction_works(|b, ext| {
			with_externalities(ext, || {
				execute_block(b);
			});
		});
	}

	#[test]
	fn block_import_with_transaction_works_wasm() {
		block_import_with_transaction_works(|b, ext| {
			WasmExecutor::new().call(ext, 8, &WASM_CODE, "Core_execute_block", &b.encode()).unwrap();
		})
	}
}
