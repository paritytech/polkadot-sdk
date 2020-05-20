// This file is part of Substrate.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Executive Module
//!
//! The Executive module acts as the orchestration layer for the runtime. It dispatches incoming
//! extrinsic calls to the respective modules in the runtime.
//!
//! ## Overview
//!
//! The executive module is not a typical pallet providing functionality around a specific feature.
//! It is a cross-cutting framework component for the FRAME. It works in conjunction with the
//! [FRAME System module](../frame_system/index.html) to perform these cross-cutting functions.
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
//! - `Executive`: Type that can be used to make the FRAME available from the runtime.
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
//! # use sp_runtime::generic;
//! # use frame_executive as executive;
//! # pub struct UncheckedExtrinsic {};
//! # pub struct Header {};
//! # type Context = frame_system::ChainContext<Runtime>;
//! # pub type Block = generic::Block<Header, UncheckedExtrinsic>;
//! # pub type Balances = u64;
//! # pub type AllModules = u64;
//! # pub enum Runtime {};
//! # use sp_runtime::transaction_validity::{
//! #    TransactionValidity, UnknownTransaction, TransactionSource,
//! # };
//! # use sp_runtime::traits::ValidateUnsigned;
//! # impl ValidateUnsigned for Runtime {
//! #     type Call = ();
//! #
//! #     fn validate_unsigned(_source: TransactionSource, _call: &Self::Call) -> TransactionValidity {
//! #         UnknownTransaction::NoUnsignedValidator.into()
//! #     }
//! # }
//! /// Executive: handles dispatch to the various modules.
//! pub type Executive = executive::Executive<Runtime, Block, Context, Runtime, AllModules>;
//! ```
//!
//! ### Custom `OnRuntimeUpgrade` logic
//!
//! You can add custom logic that should be called in your runtime on a runtime upgrade. This is
//! done by setting an optional generic parameter. The custom logic will be called before
//! the on runtime upgrade logic of all modules is called.
//!
//! ```
//! # use sp_runtime::generic;
//! # use frame_executive as executive;
//! # pub struct UncheckedExtrinsic {};
//! # pub struct Header {};
//! # type Context = frame_system::ChainContext<Runtime>;
//! # pub type Block = generic::Block<Header, UncheckedExtrinsic>;
//! # pub type Balances = u64;
//! # pub type AllModules = u64;
//! # pub enum Runtime {};
//! # use sp_runtime::transaction_validity::{
//! #    TransactionValidity, UnknownTransaction, TransactionSource,
//! # };
//! # use sp_runtime::traits::ValidateUnsigned;
//! # impl ValidateUnsigned for Runtime {
//! #     type Call = ();
//! #
//! #     fn validate_unsigned(_source: TransactionSource, _call: &Self::Call) -> TransactionValidity {
//! #         UnknownTransaction::NoUnsignedValidator.into()
//! #     }
//! # }
//! struct CustomOnRuntimeUpgrade;
//! impl frame_support::traits::OnRuntimeUpgrade for CustomOnRuntimeUpgrade {
//!     fn on_runtime_upgrade() -> frame_support::weights::Weight {
//!         // Do whatever you want.
//!         0
//!     }
//! }
//!
//! pub type Executive = executive::Executive<Runtime, Block, Context, Runtime, AllModules, CustomOnRuntimeUpgrade>;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::{prelude::*, marker::PhantomData};
use frame_support::{
	storage::StorageValue, weights::{GetDispatchInfo, DispatchInfo, DispatchClass},
	traits::{OnInitialize, OnFinalize, OnRuntimeUpgrade, OffchainWorker},
	dispatch::PostDispatchInfo,
};
use sp_runtime::{
	generic::Digest, ApplyExtrinsicResult,
	traits::{
		self, Header, Zero, One, Checkable, Applyable, CheckEqual, ValidateUnsigned, NumberFor,
		Block as BlockT, Dispatchable, Saturating,
	},
	transaction_validity::{TransactionValidity, TransactionSource},
};
use codec::{Codec, Encode};
use frame_system::{extrinsics_root, DigestOf};

/// Trait that can be used to execute a block.
pub trait ExecuteBlock<Block: BlockT> {
	/// Actually execute all transitions for `block`.
	fn execute_block(block: Block);
}

pub type CheckedOf<E, C> = <E as Checkable<C>>::Checked;
pub type CallOf<E, C> = <CheckedOf<E, C> as Applyable>::Call;
pub type OriginOf<E, C> = <CallOf<E, C> as Dispatchable>::Origin;

/// Main entry point for certain runtime actions as e.g. `execute_block`.
///
/// Generic parameters:
/// - `System`: Something that implements `frame_system::Trait`
/// - `Block`: The block type of the runtime
/// - `Context`: The context that is used when checking an extrinsic.
/// - `UnsignedValidator`: The unsigned transaction validator of the runtime.
/// - `AllModules`: Tuple that contains all modules. Will be used to call e.g. `on_initialize`.
/// - `OnRuntimeUpgrade`: Custom logic that should be called after a runtime upgrade. Modules are
///                       already called by `AllModules`. It will be called before all modules will
///                       be called.
pub struct Executive<System, Block, Context, UnsignedValidator, AllModules, OnRuntimeUpgrade = ()>(
	PhantomData<(System, Block, Context, UnsignedValidator, AllModules, OnRuntimeUpgrade)>
);

impl<
	System: frame_system::Trait,
	Block: traits::Block<Header=System::Header, Hash=System::Hash>,
	Context: Default,
	UnsignedValidator,
	AllModules:
		OnRuntimeUpgrade +
		OnInitialize<System::BlockNumber> +
		OnFinalize<System::BlockNumber> +
		OffchainWorker<System::BlockNumber>,
	COnRuntimeUpgrade: OnRuntimeUpgrade,
> ExecuteBlock<Block> for
	Executive<System, Block, Context, UnsignedValidator, AllModules, COnRuntimeUpgrade>
where
	Block::Extrinsic: Checkable<Context> + Codec,
	CheckedOf<Block::Extrinsic, Context>:
		Applyable +
		GetDispatchInfo,
	CallOf<Block::Extrinsic, Context>: Dispatchable<Info=DispatchInfo, PostInfo=PostDispatchInfo>,
	OriginOf<Block::Extrinsic, Context>: From<Option<System::AccountId>>,
	UnsignedValidator: ValidateUnsigned<Call=CallOf<Block::Extrinsic, Context>>,
{
	fn execute_block(block: Block) {
		Executive::<System, Block, Context, UnsignedValidator, AllModules>::execute_block(block);
	}
}

impl<
	System: frame_system::Trait,
	Block: traits::Block<Header=System::Header, Hash=System::Hash>,
	Context: Default,
	UnsignedValidator,
	AllModules:
		OnRuntimeUpgrade +
		OnInitialize<System::BlockNumber> +
		OnFinalize<System::BlockNumber> +
		OffchainWorker<System::BlockNumber>,
	COnRuntimeUpgrade: OnRuntimeUpgrade,
> Executive<System, Block, Context, UnsignedValidator, AllModules, COnRuntimeUpgrade>
where
	Block::Extrinsic: Checkable<Context> + Codec,
	CheckedOf<Block::Extrinsic, Context>:
		Applyable +
		GetDispatchInfo,
	CallOf<Block::Extrinsic, Context>: Dispatchable<Info=DispatchInfo, PostInfo=PostDispatchInfo>,
	OriginOf<Block::Extrinsic, Context>: From<Option<System::AccountId>>,
	UnsignedValidator: ValidateUnsigned<Call=CallOf<Block::Extrinsic, Context>>,
{
	/// Start the execution of a particular block.
	pub fn initialize_block(header: &System::Header) {
		let digests = Self::extract_pre_digest(&header);
		Self::initialize_block_impl(
			header.number(),
			header.parent_hash(),
			header.extrinsics_root(),
			&digests
		);
	}

	fn extract_pre_digest(header: &System::Header) -> DigestOf<System> {
		let mut digest = <DigestOf<System>>::default();
		header.digest().logs()
			.iter()
			.for_each(|d| if d.as_pre_runtime().is_some() {
				digest.push(d.clone())
			});
		digest
	}

	fn initialize_block_impl(
		block_number: &System::BlockNumber,
		parent_hash: &System::Hash,
		extrinsics_root: &System::Hash,
		digest: &Digest<System::Hash>,
	) {
		if Self::runtime_upgraded() {
			// System is not part of `AllModules`, so we need to call this manually.
			let mut weight = <frame_system::Module::<System> as OnRuntimeUpgrade>::on_runtime_upgrade();
			weight = weight.saturating_add(COnRuntimeUpgrade::on_runtime_upgrade());
			weight = weight.saturating_add(<AllModules as OnRuntimeUpgrade>::on_runtime_upgrade());
			<frame_system::Module<System>>::register_extra_weight_unchecked(weight, DispatchClass::Mandatory);
		}
		<frame_system::Module<System>>::initialize(
			block_number,
			parent_hash,
			extrinsics_root,
			digest,
			frame_system::InitKind::Full,
		);
		<frame_system::Module<System> as OnInitialize<System::BlockNumber>>::on_initialize(*block_number);
		let weight = <AllModules as OnInitialize<System::BlockNumber>>::on_initialize(*block_number)
			.saturating_add(<System::BlockExecutionWeight as frame_support::traits::Get<_>>::get());
		<frame_system::Module::<System>>::register_extra_weight_unchecked(weight, DispatchClass::Mandatory);

		frame_system::Module::<System>::note_finished_initialize();
	}

	/// Returns if the runtime was upgraded since the last time this function was called.
	fn runtime_upgraded() -> bool {
		let last = frame_system::LastRuntimeUpgrade::get();
		let current = <System::Version as frame_support::traits::Get<_>>::get();

		if last.map(|v| v.was_upgraded(&current)).unwrap_or(true) {
			frame_system::LastRuntimeUpgrade::put(
				frame_system::LastRuntimeUpgradeInfo::from(current),
			);
			true
		} else {
			false
		}
	}

	fn initial_checks(block: &Block) {
		let header = block.header();

		// Check that `parent_hash` is correct.
		let n = header.number().clone();
		assert!(
			n > System::BlockNumber::zero()
			&& <frame_system::Module<System>>::block_hash(n - System::BlockNumber::one()) == *header.parent_hash(),
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

		let batching_safeguard = sp_runtime::SignatureBatching::start();
		// execute extrinsics
		let (header, extrinsics) = block.deconstruct();
		Self::execute_extrinsics_with_book_keeping(extrinsics, *header.number());
		if !sp_runtime::SignatureBatching::verify(batching_safeguard) {
			panic!("Signature verification failed.");
		}

		// any final checks
		Self::final_checks(&header);
	}

	/// Execute given extrinsics and take care of post-extrinsics book-keeping.
	fn execute_extrinsics_with_book_keeping(extrinsics: Vec<Block::Extrinsic>, block_number: NumberFor<Block>) {
		extrinsics.into_iter().for_each(Self::apply_extrinsic_no_note);

		// post-extrinsics book-keeping
		<frame_system::Module<System>>::note_finished_extrinsics();
		<frame_system::Module<System> as OnFinalize<System::BlockNumber>>::on_finalize(block_number);
		<AllModules as OnFinalize<System::BlockNumber>>::on_finalize(block_number);
	}

	/// Finalize the block - it is up the caller to ensure that all header fields are valid
	/// except state-root.
	pub fn finalize_block() -> System::Header {
		<frame_system::Module<System>>::note_finished_extrinsics();
		let block_number = <frame_system::Module<System>>::block_number();
		<frame_system::Module<System> as OnFinalize<System::BlockNumber>>::on_finalize(block_number);
		<AllModules as OnFinalize<System::BlockNumber>>::on_finalize(block_number);

		// set up extrinsics
		<frame_system::Module<System>>::derive_extrinsics();
		<frame_system::Module<System>>::finalize()
	}

	/// Apply extrinsic outside of the block execution function.
	///
	/// This doesn't attempt to validate anything regarding the block, but it builds a list of uxt
	/// hashes.
	pub fn apply_extrinsic(uxt: Block::Extrinsic) -> ApplyExtrinsicResult {
		let encoded = uxt.encode();
		let encoded_len = encoded.len();
		Self::apply_extrinsic_with_len(uxt, encoded_len, Some(encoded))
	}

	/// Apply an extrinsic inside the block execution function.
	fn apply_extrinsic_no_note(uxt: Block::Extrinsic) {
		let l = uxt.encode().len();
		match Self::apply_extrinsic_with_len(uxt, l, None) {
			Ok(_) => (),
			Err(e) => { let err: &'static str = e.into(); panic!(err) },
		}
	}

	/// Actually apply an extrinsic given its `encoded_len`; this doesn't note its hash.
	fn apply_extrinsic_with_len(
		uxt: Block::Extrinsic,
		encoded_len: usize,
		to_note: Option<Vec<u8>>,
	) -> ApplyExtrinsicResult {
		// Verify that the signature is good.
		let xt = uxt.check(&Default::default())?;

		// We don't need to make sure to `note_extrinsic` only after we know it's going to be
		// executed to prevent it from leaking in storage since at this point, it will either
		// execute or panic (and revert storage changes).
		if let Some(encoded) = to_note {
			<frame_system::Module<System>>::note_extrinsic(encoded);
		}

		// AUDIT: Under no circumstances may this function panic from here onwards.

		// Decode parameters and dispatch
		let dispatch_info = xt.get_dispatch_info();
		let r = Applyable::apply::<UnsignedValidator>(xt, &dispatch_info, encoded_len)?;

		<frame_system::Module<System>>::note_applied_extrinsic(&r, dispatch_info);

		Ok(r.map(|_| ()).map_err(|e| e.error))
	}

	fn final_checks(header: &System::Header) {
		// remove temporaries
		let new_header = <frame_system::Module<System>>::finalize();

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
	pub fn validate_transaction(
		source: TransactionSource,
		uxt: Block::Extrinsic,
	) -> TransactionValidity {
		use sp_tracing::tracing_span;

		sp_tracing::enter_span!("validate_transaction");

		let encoded_len = tracing_span!{ "using_encoded"; uxt.using_encoded(|d| d.len()) };

		let xt = tracing_span!{ "check"; uxt.check(&Default::default())? };

		let dispatch_info = tracing_span!{ "dispatch_info"; xt.get_dispatch_info() };

		tracing_span! {
			"validate";
			xt.validate::<UnsignedValidator>(source, &dispatch_info, encoded_len)
		}
	}

	/// Start an offchain worker and generate extrinsics.
	pub fn offchain_worker(header: &System::Header) {
		// We need to keep events available for offchain workers,
		// hence we initialize the block manually.
		// OffchainWorker RuntimeApi should skip initialization.
		let digests = Self::extract_pre_digest(header);

		<frame_system::Module<System>>::initialize(
			header.number(),
			header.parent_hash(),
			header.extrinsics_root(),
			&digests,
			frame_system::InitKind::Inspection,
		);

		// Initialize logger, so the log messages are visible
		// also when running WASM.
		frame_support::debug::RuntimeLogger::init();

		<AllModules as OffchainWorker<System::BlockNumber>>::offchain_worker(
			// to maintain backward compatibility we call module offchain workers
			// with parent block number.
			header.number().saturating_sub(1.into())
		)
	}
}


#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H256;
	use sp_runtime::{
		generic::Era, Perbill, DispatchError, testing::{Digest, Header, Block},
		traits::{Header as HeaderT, BlakeTwo256, IdentityLookup, Convert, ConvertInto},
		transaction_validity::{InvalidTransaction, UnknownTransaction, TransactionValidityError},
	};
	use frame_support::{
		impl_outer_event, impl_outer_origin, parameter_types, impl_outer_dispatch,
		weights::{Weight, RuntimeDbWeight},
		traits::{Currency, LockIdentifier, LockableCurrency, WithdrawReasons, WithdrawReason},
	};
	use frame_system::{self as system, Call as SystemCall, ChainContext, LastRuntimeUpgradeInfo};
	use pallet_balances::Call as BalancesCall;
	use hex_literal::hex;
	const TEST_KEY: &[u8] = &*b":test:key:";

	mod custom {
		use frame_support::weights::{Weight, DispatchClass};

		pub trait Trait: frame_system::Trait {}

		frame_support::decl_module! {
			pub struct Module<T: Trait> for enum Call where origin: T::Origin {
				#[weight = 100]
				fn some_function(origin) {
					// NOTE: does not make any different.
					let _ = frame_system::ensure_signed(origin);
				}
				#[weight = (200, DispatchClass::Operational)]
				fn some_root_operation(origin) {
					let _ = frame_system::ensure_root(origin);
				}
				#[weight = 0]
				fn some_unsigned_message(origin) {
					let _ = frame_system::ensure_none(origin);
				}

				// module hooks.
				// one with block number arg and one without
				fn on_initialize(n: T::BlockNumber) -> Weight {
					println!("on_initialize({})", n);
					175
				}

				fn on_finalize() {
					println!("on_finalize(?)");
				}

				fn on_runtime_upgrade() -> Weight {
					sp_io::storage::set(super::TEST_KEY, "module".as_bytes());
					0
				}
			}
		}
	}

	type System = frame_system::Module<Runtime>;
	type Balances = pallet_balances::Module<Runtime>;
	type Custom = custom::Module<Runtime>;

	use pallet_balances as balances;

	impl_outer_origin! {
		pub enum Origin for Runtime { }
	}

	impl_outer_event!{
		pub enum MetaEvent for Runtime {
			system<T>,
			balances<T>,
		}
	}
	impl_outer_dispatch! {
		pub enum Call for Runtime where origin: Origin {
			frame_system::System,
			pallet_balances::Balances,
		}
	}

	#[derive(Clone, Eq, PartialEq)]
	pub struct Runtime;
	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::one();
		pub const BlockExecutionWeight: Weight = 10;
		pub const ExtrinsicBaseWeight: Weight = 5;
		pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight {
			read: 10,
			write: 100,
		};
	}
	impl frame_system::Trait for Runtime {
		type Origin = Origin;
		type Index = u64;
		type Call = Call;
		type BlockNumber = u64;
		type Hash = sp_core::H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<u64>;
		type Header = Header;
		type Event = MetaEvent;
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type DbWeight = DbWeight;
		type BlockExecutionWeight = BlockExecutionWeight;
		type ExtrinsicBaseWeight = ExtrinsicBaseWeight;
		type MaximumExtrinsicWeight = MaximumBlockWeight;
		type AvailableBlockRatio = AvailableBlockRatio;
		type MaximumBlockLength = MaximumBlockLength;
		type Version = RuntimeVersion;
		type ModuleToIndex = ();
		type AccountData = pallet_balances::AccountData<Balance>;
		type OnNewAccount = ();
		type OnKilledAccount = ();
	}

	type Balance = u64;
	parameter_types! {
		pub const ExistentialDeposit: Balance = 1;
	}
	impl pallet_balances::Trait for Runtime {
		type Balance = Balance;
		type Event = MetaEvent;
		type DustRemoval = ();
		type ExistentialDeposit = ExistentialDeposit;
		type AccountStore = System;
	}

	parameter_types! {
		pub const TransactionByteFee: Balance = 0;
	}
	impl pallet_transaction_payment::Trait for Runtime {
		type Currency = Balances;
		type OnTransactionPayment = ();
		type TransactionByteFee = TransactionByteFee;
		type WeightToFee = ConvertInto;
		type FeeMultiplierUpdate = ();
	}
	impl custom::Trait for Runtime {}

	impl ValidateUnsigned for Runtime {
		type Call = Call;

		fn pre_dispatch(_call: &Self::Call) -> Result<(), TransactionValidityError> {
			Ok(())
		}

		fn validate_unsigned(
			_source: TransactionSource,
			call: &Self::Call,
		) -> TransactionValidity {
			match call {
				Call::Balances(BalancesCall::set_balance(_, _, _)) => Ok(Default::default()),
				_ => UnknownTransaction::NoUnsignedValidator.into(),
			}
		}
	}

	pub struct RuntimeVersion;
	impl frame_support::traits::Get<sp_version::RuntimeVersion> for RuntimeVersion {
		fn get() -> sp_version::RuntimeVersion {
			RUNTIME_VERSION.with(|v| v.borrow().clone())
		}
	}

	thread_local! {
		pub static RUNTIME_VERSION: std::cell::RefCell<sp_version::RuntimeVersion> =
			Default::default();
	}

	type SignedExtra = (
		frame_system::CheckEra<Runtime>,
		frame_system::CheckNonce<Runtime>,
		frame_system::CheckWeight<Runtime>,
		pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
	);
	type AllModules = (System, Balances, Custom);
	type TestXt = sp_runtime::testing::TestXt<Call, SignedExtra>;

	// Will contain `true` when the custom runtime logic was called.
	const CUSTOM_ON_RUNTIME_KEY: &[u8] = &*b":custom:on_runtime";

	struct CustomOnRuntimeUpgrade;
	impl OnRuntimeUpgrade for CustomOnRuntimeUpgrade {
		fn on_runtime_upgrade() -> Weight {
			sp_io::storage::set(TEST_KEY, "custom_upgrade".as_bytes());
			sp_io::storage::set(CUSTOM_ON_RUNTIME_KEY, &true.encode());
			0
		}
	}

	type Executive = super::Executive<
		Runtime,
		Block<TestXt>,
		ChainContext<Runtime>,
		Runtime,
		AllModules,
		CustomOnRuntimeUpgrade
	>;

	fn extra(nonce: u64, fee: Balance) -> SignedExtra {
		(
			frame_system::CheckEra::from(Era::Immortal),
			frame_system::CheckNonce::from(nonce),
			frame_system::CheckWeight::new(),
			pallet_transaction_payment::ChargeTransactionPayment::from(fee)
		)
	}

	fn sign_extra(who: u64, nonce: u64, fee: Balance) -> Option<(u64, SignedExtra)> {
		Some((who, extra(nonce, fee)))
	}

	#[test]
	fn balance_transfer_dispatch_works() {
		let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![(1, 211)],
		}.assimilate_storage(&mut t).unwrap();
		let xt = TestXt::new(Call::Balances(BalancesCall::transfer(2, 69)), sign_extra(1, 0, 0));
		let weight = xt.get_dispatch_info().weight + <Runtime as frame_system::Trait>::ExtrinsicBaseWeight::get();
		let fee: Balance = <Runtime as pallet_transaction_payment::Trait>::WeightToFee::convert(weight);
		let mut t = sp_io::TestExternalities::new(t);
		t.execute_with(|| {
			Executive::initialize_block(&Header::new(
				1,
				H256::default(),
				H256::default(),
				[69u8; 32].into(),
				Digest::default(),
			));
			let r = Executive::apply_extrinsic(xt);
			assert!(r.is_ok());
			assert_eq!(<pallet_balances::Module<Runtime>>::total_balance(&1), 142 - fee);
			assert_eq!(<pallet_balances::Module<Runtime>>::total_balance(&2), 69);
		});
	}

	fn new_test_ext(balance_factor: Balance) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![(1, 111 * balance_factor)],
		}.assimilate_storage(&mut t).unwrap();
		t.into()
	}

	#[test]
	fn block_import_works() {
		new_test_ext(1).execute_with(|| {
			Executive::execute_block(Block {
				header: Header {
					parent_hash: [69u8; 32].into(),
					number: 1,
					state_root: hex!("409fb5a14aeb8b8c59258b503396a56dee45a0ee28a78de3e622db957425e275").into(),
					extrinsics_root: hex!("03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314").into(),
					digest: Digest { logs: vec![], },
				},
				extrinsics: vec![],
			});
		});
	}

	#[test]
	#[should_panic]
	fn block_import_of_bad_state_root_fails() {
		new_test_ext(1).execute_with(|| {
			Executive::execute_block(Block {
				header: Header {
					parent_hash: [69u8; 32].into(),
					number: 1,
					state_root: [0u8; 32].into(),
					extrinsics_root: hex!("03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314").into(),
					digest: Digest { logs: vec![], },
				},
				extrinsics: vec![],
			});
		});
	}

	#[test]
	#[should_panic]
	fn block_import_of_bad_extrinsic_root_fails() {
		new_test_ext(1).execute_with(|| {
			Executive::execute_block(Block {
				header: Header {
					parent_hash: [69u8; 32].into(),
					number: 1,
					state_root: hex!("49cd58a254ccf6abc4a023d9a22dcfc421e385527a250faec69f8ad0d8ed3e48").into(),
					extrinsics_root: [0u8; 32].into(),
					digest: Digest { logs: vec![], },
				},
				extrinsics: vec![],
			});
		});
	}

	#[test]
	fn bad_extrinsic_not_inserted() {
		let mut t = new_test_ext(1);
		// bad nonce check!
		let xt = TestXt::new(Call::Balances(BalancesCall::transfer(33, 69)), sign_extra(1, 30, 0));
		t.execute_with(|| {
			Executive::initialize_block(&Header::new(
				1,
				H256::default(),
				H256::default(),
				[69u8; 32].into(),
				Digest::default(),
			));
			assert!(Executive::apply_extrinsic(xt).is_err());
			assert_eq!(<frame_system::Module<Runtime>>::extrinsic_index(), Some(0));
		});
	}

	#[test]
	fn block_weight_limit_enforced() {
		let mut t = new_test_ext(10000);
		// given: TestXt uses the encoded len as fixed Len:
		let xt = TestXt::new(Call::Balances(BalancesCall::transfer(33, 0)), sign_extra(1, 0, 0));
		let encoded = xt.encode();
		let encoded_len = encoded.len() as Weight;
		// Block execution weight + on_initialize weight
		let base_block_weight = 175 + <Runtime as frame_system::Trait>::BlockExecutionWeight::get();
		let limit = AvailableBlockRatio::get() * MaximumBlockWeight::get() - base_block_weight;
		let num_to_exhaust_block = limit / (encoded_len + 5);
		t.execute_with(|| {
			Executive::initialize_block(&Header::new(
				1,
				H256::default(),
				H256::default(),
				[69u8; 32].into(),
				Digest::default(),
			));
			// Base block execution weight + `on_initialize` weight from the custom module.
			assert_eq!(<frame_system::Module<Runtime>>::all_extrinsics_weight().total(), base_block_weight);

			for nonce in 0..=num_to_exhaust_block {
				let xt = TestXt::new(
					Call::Balances(BalancesCall::transfer(33, 0)), sign_extra(1, nonce.into(), 0),
				);
				let res = Executive::apply_extrinsic(xt);
				if nonce != num_to_exhaust_block {
					assert!(res.is_ok());
					assert_eq!(
						<frame_system::Module<Runtime>>::all_extrinsics_weight().total(),
						//--------------------- on_initialize + block_execution + extrinsic_base weight
						(encoded_len + 5) * (nonce + 1) + base_block_weight,
					);
					assert_eq!(<frame_system::Module<Runtime>>::extrinsic_index(), Some(nonce as u32 + 1));
				} else {
					assert_eq!(res, Err(InvalidTransaction::ExhaustsResources.into()));
				}
			}
		});
	}

	#[test]
	fn block_weight_and_size_is_stored_per_tx() {
		let xt = TestXt::new(Call::Balances(BalancesCall::transfer(33, 0)), sign_extra(1, 0, 0));
		let x1 = TestXt::new(Call::Balances(BalancesCall::transfer(33, 0)), sign_extra(1, 1, 0));
		let x2 = TestXt::new(Call::Balances(BalancesCall::transfer(33, 0)), sign_extra(1, 2, 0));
		let len = xt.clone().encode().len() as u32;
		let mut t = new_test_ext(1);
		t.execute_with(|| {
			assert_eq!(<frame_system::Module<Runtime>>::all_extrinsics_weight().total(), 0);
			assert_eq!(<frame_system::Module<Runtime>>::all_extrinsics_len(), 0);

			assert!(Executive::apply_extrinsic(xt.clone()).unwrap().is_ok());
			assert!(Executive::apply_extrinsic(x1.clone()).unwrap().is_ok());
			assert!(Executive::apply_extrinsic(x2.clone()).unwrap().is_ok());

			// default weight for `TestXt` == encoded length.
			assert_eq!(
				<frame_system::Module<Runtime>>::all_extrinsics_weight().total(),
				3 * (len as Weight + <Runtime as frame_system::Trait>::ExtrinsicBaseWeight::get()),
			);
			assert_eq!(<frame_system::Module<Runtime>>::all_extrinsics_len(), 3 * len);

			let _ = <frame_system::Module<Runtime>>::finalize();

			assert_eq!(<frame_system::Module<Runtime>>::all_extrinsics_weight().total(), 0);
			assert_eq!(<frame_system::Module<Runtime>>::all_extrinsics_len(), 0);
		});
	}

	#[test]
	fn validate_unsigned() {
		let xt = TestXt::new(Call::Balances(BalancesCall::set_balance(33, 69, 69)), None);
		let mut t = new_test_ext(1);

		t.execute_with(|| {
			assert_eq!(
				Executive::validate_transaction(TransactionSource::InBlock, xt.clone()),
				Ok(Default::default()),
			);
			assert_eq!(Executive::apply_extrinsic(xt), Ok(Err(DispatchError::BadOrigin)));
		});
	}

	#[test]
	fn can_pay_for_tx_fee_on_full_lock() {
		let id: LockIdentifier = *b"0       ";
		let execute_with_lock = |lock: WithdrawReasons| {
			let mut t = new_test_ext(1);
			t.execute_with(|| {
				<pallet_balances::Module<Runtime> as LockableCurrency<Balance>>::set_lock(
					id,
					&1,
					110,
					lock,
				);
				let xt = TestXt::new(
					Call::System(SystemCall::remark(vec![1u8])),
					sign_extra(1, 0, 0),
				);
				let weight = xt.get_dispatch_info().weight
					+ <Runtime as frame_system::Trait>::ExtrinsicBaseWeight::get();
				let fee: Balance = <Runtime as pallet_transaction_payment::Trait>::WeightToFee::convert(weight);
				Executive::initialize_block(&Header::new(
					1,
					H256::default(),
					H256::default(),
					[69u8; 32].into(),
					Digest::default(),
				));

				if lock == WithdrawReasons::except(WithdrawReason::TransactionPayment) {
					assert!(Executive::apply_extrinsic(xt).unwrap().is_ok());
					// tx fee has been deducted.
					assert_eq!(<pallet_balances::Module<Runtime>>::total_balance(&1), 111 - fee);
				} else {
					assert_eq!(
						Executive::apply_extrinsic(xt),
						Err(InvalidTransaction::Payment.into()),
					);
					assert_eq!(<pallet_balances::Module<Runtime>>::total_balance(&1), 111);
				}
			});
		};

		execute_with_lock(WithdrawReasons::all());
		execute_with_lock(WithdrawReasons::except(WithdrawReason::TransactionPayment));
	}

	#[test]
	fn block_hooks_weight_is_stored() {
		new_test_ext(1).execute_with(|| {

			Executive::initialize_block(&Header::new_from_number(1));
			// NOTE: might need updates over time if new weights are introduced.
			// For now it only accounts for the base block execution weight and
			// the `on_initialize` weight defined in the custom test module.
			assert_eq!(<frame_system::Module<Runtime>>::all_extrinsics_weight().total(), 175 + 10);
		})
	}

	#[test]
	fn runtime_upgraded_should_work() {
		new_test_ext(1).execute_with(|| {
			RUNTIME_VERSION.with(|v| *v.borrow_mut() = Default::default());
			// It should be added at genesis
			assert!(frame_system::LastRuntimeUpgrade::exists());
			assert!(!Executive::runtime_upgraded());

			RUNTIME_VERSION.with(|v| *v.borrow_mut() = sp_version::RuntimeVersion {
				spec_version: 1,
				..Default::default()
			});
			assert!(Executive::runtime_upgraded());
			assert_eq!(
				Some(LastRuntimeUpgradeInfo { spec_version: 1.into(), spec_name: "".into() }),
				frame_system::LastRuntimeUpgrade::get(),
			);

			RUNTIME_VERSION.with(|v| *v.borrow_mut() = sp_version::RuntimeVersion {
				spec_version: 1,
				spec_name: "test".into(),
				..Default::default()
			});
			assert!(Executive::runtime_upgraded());
			assert_eq!(
				Some(LastRuntimeUpgradeInfo { spec_version: 1.into(), spec_name: "test".into() }),
				frame_system::LastRuntimeUpgrade::get(),
			);

			RUNTIME_VERSION.with(|v| *v.borrow_mut() = sp_version::RuntimeVersion {
				spec_version: 1,
				spec_name: "test".into(),
				impl_version: 2,
				..Default::default()
			});
			assert!(!Executive::runtime_upgraded());

			frame_system::LastRuntimeUpgrade::take();
			assert!(Executive::runtime_upgraded());
			assert_eq!(
				Some(LastRuntimeUpgradeInfo { spec_version: 1.into(), spec_name: "test".into() }),
				frame_system::LastRuntimeUpgrade::get(),
			);
		})
	}

	#[test]
	fn last_runtime_upgrade_was_upgraded_works() {
		let test_data = vec![
			(0, "", 1, "", true),
			(1, "", 1, "", false),
			(1, "", 1, "test", true),
			(1, "", 0, "", false),
			(1, "", 0, "test", true),
		];

		for (spec_version, spec_name, c_spec_version, c_spec_name, result) in test_data {
			let current = sp_version::RuntimeVersion {
				spec_version: c_spec_version,
				spec_name: c_spec_name.into(),
				..Default::default()
			};

			let last = LastRuntimeUpgradeInfo {
				spec_version: spec_version.into(),
				spec_name: spec_name.into(),
			};

			assert_eq!(result, last.was_upgraded(&current));
		}
	}

	#[test]
	fn custom_runtime_upgrade_is_called_before_modules() {
		new_test_ext(1).execute_with(|| {
			// Make sure `on_runtime_upgrade` is called.
			RUNTIME_VERSION.with(|v| *v.borrow_mut() = sp_version::RuntimeVersion {
				spec_version: 1,
				..Default::default()
			});

			Executive::initialize_block(&Header::new(
				1,
				H256::default(),
				H256::default(),
				[69u8; 32].into(),
				Digest::default(),
			));

			assert_eq!(&sp_io::storage::get(TEST_KEY).unwrap()[..], *b"module");
			assert_eq!(sp_io::storage::get(CUSTOM_ON_RUNTIME_KEY).unwrap(), true.encode());
		});
	}
}
