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

//! # Executive Module
//!
//! The Executive module acts as the orchestration layer for the runtime. It dispatches incoming
//! extrinsic calls to the respective modules in the runtime.
//!
//! ## Overview
//!
//! The executive module is not a typical SRML module providing functionality around a specific feature.
//! It is a cross-cutting framework component for the SRML. It works in conjunction with the
//! [SRML System module](../srml_system/index.html) to perform these cross-cutting functions.
//!
//! The Executive module provides functions to:
//!
//! - Check transaction validity.
//! - Initialize a block.
//! - Apply extrinsics.
//! - Execute a block.
//! - Finalize a block.
//! - Start an off-chain worker.
//!
//! ### Implementations
//!
//! The Executive module provides the following implementations:
//!
//! - `ExecuteBlock`: Trait that can be used to execute a block.
//! - `Executive`: Type that can be used to make the SRML available from the runtime.
//!
//! ## Usage
//!
//! The default Substrate node template declares the [`Executive`](./struct.Executive.html) type in its library.
//!
//! ### Example
//!
//! `Executive` type declaration from the node template.
//!
//! ```
//! # use primitives::generic;
//! # use srml_executive as executive;
//! # pub struct UncheckedExtrinsic {};
//! # pub struct Header {};
//! # type Context = system::ChainContext<Runtime>;
//! # pub type Block = generic::Block<Header, UncheckedExtrinsic>;
//! # pub type Balances = u64;
//! # pub type AllModules = u64;
//! # pub enum Runtime {};
//! # use primitives::transaction_validity::TransactionValidity;
//! # use primitives::traits::ValidateUnsigned;
//! # impl ValidateUnsigned for Runtime {
//! # 	type Call = ();
//! #
//! # 	fn validate_unsigned(_call: &Self::Call) -> TransactionValidity {
//! # 		TransactionValidity::Invalid(0)
//! # 	}
//! # }
//! /// Executive: handles dispatch to the various modules.
//! pub type Executive = executive::Executive<Runtime, Block, Context, Balances, Runtime, AllModules>;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

use rstd::prelude::*;
use rstd::marker::PhantomData;
use rstd::result;
use primitives::{generic::Digest, traits::{
	self, Header, Zero, One, Checkable, Applyable, CheckEqual, OnFinalize,
	OnInitialize, NumberFor, Block as BlockT, OffchainWorker,
	ValidateUnsigned,
}};
use srml_support::{Dispatchable, traits::MakePayment};
use parity_codec::{Codec, Encode};
use system::{extrinsics_root, DigestOf};
use primitives::{ApplyOutcome, ApplyError};
use primitives::transaction_validity::{TransactionValidity, TransactionPriority, TransactionLongevity};
use primitives::weights::{Weighable, MAX_TRANSACTIONS_WEIGHT};

mod mock;
mod tests;

mod internal {

	pub enum ApplyError {
		BadSignature(&'static str),
		Stale,
		Future,
		CantPay,
		FullBlock,
	}

	pub enum ApplyOutcome {
		Success,
		Fail(&'static str),
	}
}

/// Trait that can be used to execute a block.
pub trait ExecuteBlock<Block: BlockT> {
	/// Actually execute all transitions for `block`.
	fn execute_block(block: Block);
}

pub type CheckedOf<E, C> = <E as Checkable<C>>::Checked;
pub type CallOf<E, C> = <CheckedOf<E, C> as Applyable>::Call;
pub type OriginOf<E, C> = <CallOf<E, C> as Dispatchable>::Origin;

pub struct Executive<System, Block, Context, Payment, UnsignedValidator, AllModules>(
	PhantomData<(System, Block, Context, Payment, UnsignedValidator, AllModules)>
);

impl<
	System: system::Trait,
	Block: traits::Block<Header=System::Header, Hash=System::Hash>,
	Context: Default,
	Payment: MakePayment<System::AccountId>,
	UnsignedValidator,
	AllModules: OnInitialize<System::BlockNumber> + OnFinalize<System::BlockNumber> + OffchainWorker<System::BlockNumber>,
> ExecuteBlock<Block> for Executive<System, Block, Context, Payment, UnsignedValidator, AllModules>
where
	Block::Extrinsic: Checkable<Context> + Codec,
	CheckedOf<Block::Extrinsic, Context>: Applyable<Index=System::Index, AccountId=System::AccountId> + Weighable,
	CallOf<Block::Extrinsic, Context>: Dispatchable,
	OriginOf<Block::Extrinsic, Context>: From<Option<System::AccountId>>,
	UnsignedValidator: ValidateUnsigned<Call=CallOf<Block::Extrinsic, Context>>,
{
	fn execute_block(block: Block) {
		Executive::<System, Block, Context, Payment, UnsignedValidator, AllModules>::execute_block(block);
	}
}

impl<
	System: system::Trait,
	Block: traits::Block<Header=System::Header, Hash=System::Hash>,
	Context: Default,
	Payment: MakePayment<System::AccountId>,
	UnsignedValidator,
	AllModules: OnInitialize<System::BlockNumber> + OnFinalize<System::BlockNumber> + OffchainWorker<System::BlockNumber>,
> Executive<System, Block, Context, Payment, UnsignedValidator, AllModules>
where
	Block::Extrinsic: Checkable<Context> + Codec,
	CheckedOf<Block::Extrinsic, Context>: Applyable<Index=System::Index, AccountId=System::AccountId> + Weighable,
	CallOf<Block::Extrinsic, Context>: Dispatchable,
	OriginOf<Block::Extrinsic, Context>: From<Option<System::AccountId>>,
	UnsignedValidator: ValidateUnsigned<Call=CallOf<Block::Extrinsic, Context>>,
{
	/// Start the execution of a particular block.
	pub fn initialize_block(header: &System::Header) {
		let mut digests = <DigestOf<System>>::default();
		header.digest().logs().iter().for_each(|d| if d.as_pre_runtime().is_some() { digests.push(d.clone()) });
		Self::initialize_block_impl(header.number(), header.parent_hash(), header.extrinsics_root(), &digests);
	}

	fn initialize_block_impl(
		block_number: &System::BlockNumber,
		parent_hash: &System::Hash,
		extrinsics_root: &System::Hash,
		digest: &Digest<System::Hash>,
	) {
		<system::Module<System>>::initialize(block_number, parent_hash, extrinsics_root, digest);
		<AllModules as OnInitialize<System::BlockNumber>>::on_initialize(*block_number);
	}

	fn initial_checks(block: &Block) {
		let header = block.header();

		// Check that `parent_hash` is correct.
		let n = header.number().clone();
		assert!(
			n > System::BlockNumber::zero()
			&& <system::Module<System>>::block_hash(n - System::BlockNumber::one()) == *header.parent_hash(),
			"Parent hash should be valid."
		);

		// Check that transaction trie root represents the transactions.
		let xts_root = extrinsics_root::<System::Hashing, _>(&block.extrinsics());
		header.extrinsics_root().check_equal(&xts_root);
		assert!(header.extrinsics_root() == &xts_root, "Transaction trie root must be valid.");
	}

	/// Actually execute all transitions for `block`.
	pub fn execute_block(block: Block) {
		Self::initialize_block(block.header());

		// any initial checks
		Self::initial_checks(&block);

		// execute extrinsics
		let (header, extrinsics) = block.deconstruct();
		Self::execute_extrinsics_with_book_keeping(extrinsics, *header.number());

		// any final checks
		Self::final_checks(&header);
	}

	/// Execute given extrinsics and take care of post-extrinsics book-keeping.
	fn execute_extrinsics_with_book_keeping(extrinsics: Vec<Block::Extrinsic>, block_number: NumberFor<Block>) {

		extrinsics.into_iter().for_each(Self::apply_extrinsic_no_note);

		// post-extrinsics book-keeping
		<system::Module<System>>::note_finished_extrinsics();
		<AllModules as OnFinalize<System::BlockNumber>>::on_finalize(block_number);
	}

	/// Finalize the block - it is up the caller to ensure that all header fields are valid
	/// except state-root.
	pub fn finalize_block() -> System::Header {
		<system::Module<System>>::note_finished_extrinsics();
		<AllModules as OnFinalize<System::BlockNumber>>::on_finalize(<system::Module<System>>::block_number());

		// set up extrinsics
		<system::Module<System>>::derive_extrinsics();
		<system::Module<System>>::finalize()
	}

	/// Apply extrinsic outside of the block execution function.
	/// This doesn't attempt to validate anything regarding the block, but it builds a list of uxt
	/// hashes.
	pub fn apply_extrinsic(uxt: Block::Extrinsic) -> result::Result<ApplyOutcome, ApplyError> {
		let encoded = uxt.encode();
		let encoded_len = encoded.len();
		match Self::apply_extrinsic_with_len(uxt, encoded_len, Some(encoded)) {
			Ok(internal::ApplyOutcome::Success) => Ok(ApplyOutcome::Success),
			Ok(internal::ApplyOutcome::Fail(_)) => Ok(ApplyOutcome::Fail),
			Err(internal::ApplyError::CantPay) => Err(ApplyError::CantPay),
			Err(internal::ApplyError::BadSignature(_)) => Err(ApplyError::BadSignature),
			Err(internal::ApplyError::Stale) => Err(ApplyError::Stale),
			Err(internal::ApplyError::Future) => Err(ApplyError::Future),
			Err(internal::ApplyError::FullBlock) => Err(ApplyError::FullBlock),
		}
	}

	/// Apply an extrinsic inside the block execution function.
	fn apply_extrinsic_no_note(uxt: Block::Extrinsic) {
		let l = uxt.encode().len();
		match Self::apply_extrinsic_with_len(uxt, l, None) {
			Ok(internal::ApplyOutcome::Success) => (),
			Ok(internal::ApplyOutcome::Fail(e)) => runtime_io::print(e),
			Err(internal::ApplyError::CantPay) => panic!("All extrinsics should have sender able to pay their fees"),
			Err(internal::ApplyError::BadSignature(_)) => panic!("All extrinsics should be properly signed"),
			Err(internal::ApplyError::Stale) | Err(internal::ApplyError::Future) => panic!("All extrinsics should have the correct nonce"),
			Err(internal::ApplyError::FullBlock) => panic!("Extrinsics should not exceed block limit"),
		}
	}

	/// Actually apply an extrinsic given its `encoded_len`; this doesn't note its hash.
	fn apply_extrinsic_with_len(
		uxt: Block::Extrinsic,
		encoded_len: usize,
		to_note: Option<Vec<u8>>,
	) -> result::Result<internal::ApplyOutcome, internal::ApplyError> {
		// Verify that the signature is good.
		let xt = uxt.check(&Default::default()).map_err(internal::ApplyError::BadSignature)?;

		// Check the weight of the block if that extrinsic is applied.
		let weight = xt.weight(encoded_len);
		if <system::Module<System>>::all_extrinsics_weight() + weight > MAX_TRANSACTIONS_WEIGHT {
			return Err(internal::ApplyError::FullBlock);
		}

		if let (Some(sender), Some(index)) = (xt.sender(), xt.index()) {
			// check index
			let expected_index = <system::Module<System>>::account_nonce(sender);
			if index != &expected_index { return Err(
				if index < &expected_index { internal::ApplyError::Stale } else { internal::ApplyError::Future }
			) }
			// pay any fees
			let weight_multiplier = <system::Module<System>>::next_weight_multiplier();
			Payment::make_payment(sender, weight_multiplier.apply_to(weight)).map_err(|_| internal::ApplyError::CantPay)?;

			// AUDIT: Under no circumstances may this function panic from here onwards.
			// FIXME: ensure this at compile-time (such as by not defining a panic function, forcing
			// a linker error unless the compiler can prove it cannot be called).
			// increment nonce in storage
			<system::Module<System>>::inc_account_nonce(sender);
		}

		// Make sure to `note_extrinsic` only after we know it's going to be executed
		// to prevent it from leaking in storage.
		if let Some(encoded) = to_note {
			<system::Module<System>>::note_extrinsic(encoded);
		}

		// Decode parameters and dispatch
		let (f, s) = xt.deconstruct();
		let r = f.dispatch(s.into());
		<system::Module<System>>::note_applied_extrinsic(&r, weight);

		r.map(|_| internal::ApplyOutcome::Success).or_else(|e| match e {
			primitives::BLOCK_FULL => Err(internal::ApplyError::FullBlock),
			e => Ok(internal::ApplyOutcome::Fail(e))
		})
	}

	fn final_checks(header: &System::Header) {
		// remove temporaries
		let new_header = <system::Module<System>>::finalize();

		// check digest
		assert_eq!(
			header.digest().logs().len(),
			new_header.digest().logs().len(),
			"Number of digest items must match that calculated."
		);
		let items_zip = header.digest().logs().iter().zip(new_header.digest().logs().iter());
		for (header_item, computed_item) in items_zip {
			header_item.check_equal(&computed_item);
			assert!(header_item == computed_item, "Digest item must match that calculated.");
		}

		// check storage root.
		let storage_root = new_header.state_root();
		header.state_root().check_equal(&storage_root);
		assert!(header.state_root() == storage_root, "Storage root must match that calculated.");
	}

	/// Check a given signed transaction for validity. This doesn't execute any
	/// side-effects; it merely checks whether the transaction would panic if it were included or not.
	///
	/// Changes made to storage should be discarded.
	pub fn validate_transaction(uxt: Block::Extrinsic) -> TransactionValidity {
		// Note errors > 0 are from ApplyError
		const UNKNOWN_ERROR: i8 = -127;
		const MISSING_SENDER: i8 = -20;
		const INVALID_INDEX: i8 = -10;

		let encoded_len = uxt.encode().len();

		let xt = match uxt.check(&Default::default()) {
			// Checks out. Carry on.
			Ok(xt) => xt,
			// An unknown account index implies that the transaction may yet become valid.
			Err("invalid account index") => return TransactionValidity::Unknown(INVALID_INDEX),
			// Technically a bad signature could also imply an out-of-date account index, but
			// that's more of an edge case.
			Err(primitives::BAD_SIGNATURE) => return TransactionValidity::Invalid(ApplyError::BadSignature as i8),
			Err(_) => return TransactionValidity::Invalid(UNKNOWN_ERROR),
		};

		match (xt.sender(), xt.index()) {
			(Some(sender), Some(index)) => {
				let weight = xt.weight(encoded_len);

				// pay any fees
				let weight_multiplier = <system::Module<System>>::next_weight_multiplier();
				if Payment::make_payment(sender, weight_multiplier.apply_to(weight)).is_err() {
					return TransactionValidity::Invalid(ApplyError::CantPay as i8)
				}

				// check index
				let expected_index = <system::Module<System>>::account_nonce(sender);
				if index < &expected_index {
					return TransactionValidity::Invalid(ApplyError::Stale as i8)
				}

				let index = *index;
				let provides = vec![(sender, index).encode()];
				let requires = if expected_index < index {
					vec![(sender, index - One::one()).encode()]
				} else {
					vec![]
				};

				TransactionValidity::Valid {
					priority: encoded_len as TransactionPriority,
					requires,
					provides,
					longevity: TransactionLongevity::max_value(),
					propagate: true,
				}
			},
			(None, None) => UnsignedValidator::validate_unsigned(&xt.deconstruct().0),
			(Some(_), None) => TransactionValidity::Invalid(INVALID_INDEX),
			(None, Some(_)) => TransactionValidity::Invalid(MISSING_SENDER),
		}
	}

	/// Start an offchain worker and generate extrinsics.
	pub fn offchain_worker(n: System::BlockNumber) {
		<AllModules as OffchainWorker<System::BlockNumber>>::generate_extrinsics(n)
	}
}
