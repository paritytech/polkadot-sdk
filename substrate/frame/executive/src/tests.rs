// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Test the `frame-executive` crate.

use super::*;

use sp_core::H256;
use sp_runtime::{
	generic::{DigestItem, Era},
	testing::{Block, Digest, Header},
	traits::{Block as BlockT, Header as HeaderT},
	transaction_validity::{
		InvalidTransaction, TransactionValidityError, UnknownTransaction, ValidTransaction,
	},
	BuildStorage, DispatchError,
};

use frame_support::{
	assert_err, assert_ok, derive_impl,
	migrations::MultiStepMigrator,
	pallet_prelude::*,
	parameter_types,
	traits::{fungible, ConstU8, Currency, IsInherent},
	weights::{ConstantMultiplier, IdentityFee, RuntimeDbWeight, Weight, WeightMeter, WeightToFee},
};
use frame_system::{pallet_prelude::*, ChainContext, LastRuntimeUpgrade, LastRuntimeUpgradeInfo};
use pallet_balances::Call as BalancesCall;
use pallet_transaction_payment::CurrencyAdapter;

const TEST_KEY: &[u8] = b":test:key:";

#[frame_support::pallet(dev_mode)]
mod custom {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// module hooks.
		// one with block number arg and one without
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			Weight::from_parts(175, 0)
		}

		fn on_idle(_: BlockNumberFor<T>, _: Weight) -> Weight {
			Weight::from_parts(175, 0)
		}

		fn on_poll(_: BlockNumberFor<T>, _: &mut WeightMeter) {}

		fn on_finalize(_: BlockNumberFor<T>) {}

		fn on_runtime_upgrade() -> Weight {
			sp_io::storage::set(super::TEST_KEY, "module".as_bytes());
			Weight::from_parts(200, 0)
		}

		fn offchain_worker(n: BlockNumberFor<T>) {
			assert_eq!(BlockNumberFor::<T>::from(1u32), n);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn some_function(origin: OriginFor<T>) -> DispatchResult {
			// NOTE: does not make any difference.
			frame_system::ensure_signed(origin)?;
			Ok(())
		}

		#[pallet::weight((200, DispatchClass::Operational))]
		pub fn some_root_operation(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			Ok(())
		}

		pub fn some_unsigned_message(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_none(origin)?;
			Ok(())
		}

		pub fn allowed_unsigned(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			Ok(())
		}

		pub fn unallowed_unsigned(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			Ok(())
		}

		#[pallet::weight((0, DispatchClass::Mandatory))]
		pub fn inherent(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_none(origin)?;
			Ok(())
		}

		pub fn calculate_storage_root(_origin: OriginFor<T>) -> DispatchResult {
			let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
			sp_io::storage::set("storage_root".as_bytes(), &root);
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;

		type Error = sp_inherents::MakeFatalError<()>;

		const INHERENT_IDENTIFIER: [u8; 8] = *b"test1234";

		fn create_inherent(_data: &InherentData) -> Option<Self::Call> {
			None
		}

		fn is_inherent(call: &Self::Call) -> bool {
			*call == Call::<T>::inherent {}
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		// Inherent call is accepted for being dispatched
		fn pre_dispatch(call: &Self::Call) -> Result<(), TransactionValidityError> {
			match call {
				Call::allowed_unsigned { .. } => Ok(()),
				Call::inherent { .. } => Ok(()),
				_ => Err(UnknownTransaction::NoUnsignedValidator.into()),
			}
		}

		// Inherent call is not validated as unsigned
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::allowed_unsigned { .. } => Ok(Default::default()),
				_ => UnknownTransaction::NoUnsignedValidator.into(),
			}
		}
	}
}

#[frame_support::pallet(dev_mode)]
mod custom2 {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// module hooks.
		// one with block number arg and one without
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			assert!(
				!MockedSystemCallbacks::pre_inherent_called(),
				"Pre inherent hook goes after on_initialize"
			);

			Weight::from_parts(0, 0)
		}

		fn on_idle(_: BlockNumberFor<T>, _: Weight) -> Weight {
			assert!(
				MockedSystemCallbacks::post_transactions_called(),
				"Post transactions hook goes before after on_idle"
			);
			Weight::from_parts(0, 0)
		}

		fn on_finalize(_: BlockNumberFor<T>) {
			assert!(
				MockedSystemCallbacks::post_transactions_called(),
				"Post transactions hook goes before after on_finalize"
			);
		}

		fn on_runtime_upgrade() -> Weight {
			sp_io::storage::set(super::TEST_KEY, "module".as_bytes());
			Weight::from_parts(0, 0)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn allowed_unsigned(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			Ok(())
		}

		pub fn some_call(_: OriginFor<T>) -> DispatchResult {
			assert!(MockedSystemCallbacks::post_inherent_called());
			assert!(!MockedSystemCallbacks::post_transactions_called());
			assert!(System::inherents_applied());

			Ok(())
		}

		#[pallet::weight({0})]
		pub fn optional_inherent(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_none(origin)?;

			assert!(MockedSystemCallbacks::pre_inherent_called());
			assert!(!MockedSystemCallbacks::post_inherent_called(), "Should not already be called");
			assert!(!System::inherents_applied());

			Ok(())
		}

		#[pallet::weight((0, DispatchClass::Mandatory))]
		pub fn inherent(origin: OriginFor<T>) -> DispatchResult {
			frame_system::ensure_none(origin)?;

			assert!(MockedSystemCallbacks::pre_inherent_called());
			assert!(!MockedSystemCallbacks::post_inherent_called(), "Should not already be called");
			assert!(!System::inherents_applied());

			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;

		type Error = sp_inherents::MakeFatalError<()>;

		const INHERENT_IDENTIFIER: [u8; 8] = *b"test1235";

		fn create_inherent(_data: &InherentData) -> Option<Self::Call> {
			None
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::<T>::inherent {} | Call::<T>::optional_inherent {})
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		// Inherent call is accepted for being dispatched
		fn pre_dispatch(call: &Self::Call) -> Result<(), TransactionValidityError> {
			match call {
				Call::allowed_unsigned { .. } |
				Call::optional_inherent { .. } |
				Call::inherent { .. } => Ok(()),
				_ => Err(UnknownTransaction::NoUnsignedValidator.into()),
			}
		}

		// Inherent call is not validated as unsigned
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::allowed_unsigned { .. } => Ok(Default::default()),
				_ => UnknownTransaction::NoUnsignedValidator.into(),
			}
		}
	}
}

frame_support::construct_runtime!(
	pub struct Runtime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>},
		Custom: custom::{Pallet, Call, ValidateUnsigned, Inherent},
		Custom2: custom2::{Pallet, Call, ValidateUnsigned, Inherent},
	}
);

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::builder()
			.base_block(Weight::from_parts(10, 0))
			.for_class(DispatchClass::all(), |weights| weights.base_extrinsic = Weight::from_parts(5, 0))
			.for_class(DispatchClass::non_mandatory(), |weights| weights.max_total = Weight::from_parts(1024, u64::MAX).into())
			.build_or_panic();
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 10,
		write: 100,
	};
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type BlockWeights = BlockWeights;
	type RuntimeOrigin = RuntimeOrigin;
	type Nonce = u64;
	type RuntimeCall = RuntimeCall;
	type Block = TestBlock;
	type RuntimeEvent = RuntimeEvent;
	type Version = RuntimeVersion;
	type AccountData = pallet_balances::AccountData<Balance>;
	type PreInherents = MockedSystemCallbacks;
	type PostInherents = MockedSystemCallbacks;
	type PostTransactions = MockedSystemCallbacks;
	type MultiBlockMigrator = MockedModeGetter;
}

type Balance = u64;

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type AccountStore = System;
}

parameter_types! {
	pub const TransactionByteFee: Balance = 0;
}
impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = CurrencyAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = ();
}

impl custom::Config for Runtime {}
impl custom2::Config for Runtime {}

pub struct RuntimeVersion;
impl frame_support::traits::Get<sp_version::RuntimeVersion> for RuntimeVersion {
	fn get() -> sp_version::RuntimeVersion {
		RuntimeVersionTestValues::get().clone()
	}
}

parameter_types! {
	pub static RuntimeVersionTestValues: sp_version::RuntimeVersion =
		Default::default();
}

type SignedExtra = (
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);
type TestXt = sp_runtime::testing::TestXt<RuntimeCall, SignedExtra>;
type TestBlock = Block<TestXt>;

// Will contain `true` when the custom runtime logic was called.
const CUSTOM_ON_RUNTIME_KEY: &[u8] = b":custom:on_runtime";

struct CustomOnRuntimeUpgrade;
impl OnRuntimeUpgrade for CustomOnRuntimeUpgrade {
	fn on_runtime_upgrade() -> Weight {
		sp_io::storage::set(TEST_KEY, "custom_upgrade".as_bytes());
		sp_io::storage::set(CUSTOM_ON_RUNTIME_KEY, &true.encode());
		System::deposit_event(frame_system::Event::CodeUpdated);

		assert_eq!(0, System::last_runtime_upgrade_spec_version());

		Weight::from_parts(100, 0)
	}
}

type Executive = super::Executive<
	Runtime,
	Block<TestXt>,
	ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	CustomOnRuntimeUpgrade,
>;

parameter_types! {
	pub static SystemCallbacksCalled: u32 = 0;
}

pub struct MockedSystemCallbacks;
impl PreInherents for MockedSystemCallbacks {
	fn pre_inherents() {
		assert_eq!(SystemCallbacksCalled::get(), 0);
		SystemCallbacksCalled::set(1);
		// Change the storage to modify the root hash:
		frame_support::storage::unhashed::put(b":pre_inherent", b"0");
	}
}

impl PostInherents for MockedSystemCallbacks {
	fn post_inherents() {
		assert_eq!(SystemCallbacksCalled::get(), 1);
		SystemCallbacksCalled::set(2);
		// Change the storage to modify the root hash:
		frame_support::storage::unhashed::put(b":post_inherent", b"0");
	}
}

impl PostTransactions for MockedSystemCallbacks {
	fn post_transactions() {
		assert_eq!(SystemCallbacksCalled::get(), 2);
		SystemCallbacksCalled::set(3);
		// Change the storage to modify the root hash:
		frame_support::storage::unhashed::put(b":post_transaction", b"0");
	}
}

impl MockedSystemCallbacks {
	fn pre_inherent_called() -> bool {
		SystemCallbacksCalled::get() >= 1
	}

	fn post_inherent_called() -> bool {
		SystemCallbacksCalled::get() >= 2
	}

	fn post_transactions_called() -> bool {
		SystemCallbacksCalled::get() >= 3
	}

	fn reset() {
		SystemCallbacksCalled::set(0);
		frame_support::storage::unhashed::kill(b":pre_inherent");
		frame_support::storage::unhashed::kill(b":post_inherent");
		frame_support::storage::unhashed::kill(b":post_transaction");
	}
}

parameter_types! {
	pub static MbmActive: bool = false;
}

pub struct MockedModeGetter;
impl MultiStepMigrator for MockedModeGetter {
	fn ongoing() -> bool {
		MbmActive::get()
	}

	fn step() -> Weight {
		Weight::zero()
	}
}

fn extra(nonce: u64, fee: Balance) -> SignedExtra {
	(
		frame_system::CheckEra::from(Era::Immortal),
		frame_system::CheckNonce::from(nonce),
		frame_system::CheckWeight::new(),
		pallet_transaction_payment::ChargeTransactionPayment::from(fee),
	)
}

fn sign_extra(who: u64, nonce: u64, fee: Balance) -> Option<(u64, SignedExtra)> {
	Some((who, extra(nonce, fee)))
}

fn call_transfer(dest: u64, value: u64) -> RuntimeCall {
	RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value })
}

#[test]
fn balance_transfer_dispatch_works() {
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Runtime> { balances: vec![(1, 211)] }
		.assimilate_storage(&mut t)
		.unwrap();
	let xt = TestXt::new(call_transfer(2, 69), sign_extra(1, 0, 0));
	let weight = xt.get_dispatch_info().weight +
		<Runtime as frame_system::Config>::BlockWeights::get()
			.get(DispatchClass::Normal)
			.base_extrinsic;
	let fee: Balance =
		<Runtime as pallet_transaction_payment::Config>::WeightToFee::weight_to_fee(&weight);
	let mut t = sp_io::TestExternalities::new(t);
	t.execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));
		let r = Executive::apply_extrinsic(xt);
		assert!(r.is_ok());
		assert_eq!(<pallet_balances::Pallet<Runtime>>::total_balance(&1), 142 - fee);
		assert_eq!(<pallet_balances::Pallet<Runtime>>::total_balance(&2), 69);
	});
}

fn new_test_ext(balance_factor: Balance) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Runtime> { balances: vec![(1, 111 * balance_factor)] }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		SystemCallbacksCalled::set(0);
	});
	ext
}

fn new_test_ext_v0(balance_factor: Balance) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Runtime> { balances: vec![(1, 111 * balance_factor)] }
		.assimilate_storage(&mut t)
		.unwrap();
	(t, sp_runtime::StateVersion::V0).into()
}

#[test]
fn block_import_works() {
	block_import_works_inner(
		new_test_ext_v0(1),
		array_bytes::hex_n_into_unchecked(
			"4826d3bdf87dbbc883d2ab274cbe272f58ed94a904619b59953e48294d1142d2",
		),
	);
	block_import_works_inner(
		new_test_ext(1),
		array_bytes::hex_n_into_unchecked(
			"d6b465f5a50c9f8d5a6edc0f01d285a6b19030f097d3aaf1649b7be81649f118",
		),
	);
}
fn block_import_works_inner(mut ext: sp_io::TestExternalities, state_root: H256) {
	ext.execute_with(|| {
		Executive::execute_block(Block {
			header: Header {
				parent_hash: [69u8; 32].into(),
				number: 1,
				state_root,
				extrinsics_root: array_bytes::hex_n_into_unchecked(
					"03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314",
				),
				digest: Digest { logs: vec![] },
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
				extrinsics_root: array_bytes::hex_n_into_unchecked(
					"03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314",
				),
				digest: Digest { logs: vec![] },
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
				state_root: array_bytes::hex_n_into_unchecked(
					"75e7d8f360d375bbe91bcf8019c01ab6362448b4a89e3b329717eb9d910340e5",
				),
				extrinsics_root: [0u8; 32].into(),
				digest: Digest { logs: vec![] },
			},
			extrinsics: vec![],
		});
	});
}

#[test]
fn bad_extrinsic_not_inserted() {
	let mut t = new_test_ext(1);
	// bad nonce check!
	let xt = TestXt::new(call_transfer(33, 69), sign_extra(1, 30, 0));
	t.execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));
		assert_err!(
			Executive::apply_extrinsic(xt),
			TransactionValidityError::Invalid(InvalidTransaction::Future)
		);
		assert_eq!(<frame_system::Pallet<Runtime>>::extrinsic_index(), Some(0));
	});
}

#[test]
fn block_weight_limit_enforced() {
	let mut t = new_test_ext(10000);
	// given: TestXt uses the encoded len as fixed Len:
	let xt = TestXt::new(
		RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 33, value: 0 }),
		sign_extra(1, 0, 0),
	);
	let encoded = xt.encode();
	let encoded_len = encoded.len() as u64;
	// on_initialize weight + base block execution weight
	let block_weights = <Runtime as frame_system::Config>::BlockWeights::get();
	let base_block_weight = Weight::from_parts(175, 0) + block_weights.base_block;
	let limit = block_weights.get(DispatchClass::Normal).max_total.unwrap() - base_block_weight;
	let num_to_exhaust_block = limit.ref_time() / (encoded_len + 5);
	t.execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));
		// Base block execution weight + `on_initialize` weight from the custom module.
		assert_eq!(<frame_system::Pallet<Runtime>>::block_weight().total(), base_block_weight);

		for nonce in 0..=num_to_exhaust_block {
			let xt = TestXt::new(
				RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 33, value: 0 }),
				sign_extra(1, nonce.into(), 0),
			);
			let res = Executive::apply_extrinsic(xt);
			if nonce != num_to_exhaust_block {
				assert!(res.is_ok());
				assert_eq!(
					<frame_system::Pallet<Runtime>>::block_weight().total(),
					//--------------------- on_initialize + block_execution + extrinsic_base weight
					Weight::from_parts((encoded_len + 5) * (nonce + 1), 0) + base_block_weight,
				);
				assert_eq!(
					<frame_system::Pallet<Runtime>>::extrinsic_index(),
					Some(nonce as u32 + 1)
				);
			} else {
				assert_eq!(res, Err(InvalidTransaction::ExhaustsResources.into()));
			}
		}
	});
}

#[test]
fn block_weight_and_size_is_stored_per_tx() {
	let xt = TestXt::new(
		RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 33, value: 0 }),
		sign_extra(1, 0, 0),
	);
	let x1 = TestXt::new(
		RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 33, value: 0 }),
		sign_extra(1, 1, 0),
	);
	let x2 = TestXt::new(
		RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 33, value: 0 }),
		sign_extra(1, 2, 0),
	);
	let len = xt.clone().encode().len() as u32;
	let mut t = new_test_ext(1);
	t.execute_with(|| {
		// Block execution weight + on_initialize weight from custom module
		let base_block_weight = Weight::from_parts(175, 0) +
			<Runtime as frame_system::Config>::BlockWeights::get().base_block;

		Executive::initialize_block(&Header::new_from_number(1));

		assert_eq!(<frame_system::Pallet<Runtime>>::block_weight().total(), base_block_weight);
		assert_eq!(<frame_system::Pallet<Runtime>>::all_extrinsics_len(), 0);

		assert!(Executive::apply_extrinsic(xt.clone()).unwrap().is_ok());
		assert!(Executive::apply_extrinsic(x1.clone()).unwrap().is_ok());
		assert!(Executive::apply_extrinsic(x2.clone()).unwrap().is_ok());

		// default weight for `TestXt` == encoded length.
		let extrinsic_weight = Weight::from_parts(len as u64, 0) +
			<Runtime as frame_system::Config>::BlockWeights::get()
				.get(DispatchClass::Normal)
				.base_extrinsic;
		assert_eq!(
			<frame_system::Pallet<Runtime>>::block_weight().total(),
			base_block_weight + 3u64 * extrinsic_weight,
		);
		assert_eq!(<frame_system::Pallet<Runtime>>::all_extrinsics_len(), 3 * len);

		let _ = <frame_system::Pallet<Runtime>>::finalize();
		// All extrinsics length cleaned on `System::finalize`
		assert_eq!(<frame_system::Pallet<Runtime>>::all_extrinsics_len(), 0);

		// Reset to a new block.
		SystemCallbacksCalled::take();
		Executive::initialize_block(&Header::new_from_number(2));

		// Block weight cleaned up on `System::initialize`
		assert_eq!(<frame_system::Pallet<Runtime>>::block_weight().total(), base_block_weight);
	});
}

#[test]
fn validate_unsigned() {
	let valid = TestXt::new(RuntimeCall::Custom(custom::Call::allowed_unsigned {}), None);
	let invalid = TestXt::new(RuntimeCall::Custom(custom::Call::unallowed_unsigned {}), None);
	let mut t = new_test_ext(1);

	t.execute_with(|| {
		assert_eq!(
			Executive::validate_transaction(
				TransactionSource::InBlock,
				valid.clone(),
				Default::default(),
			),
			Ok(ValidTransaction::default()),
		);
		assert_eq!(
			Executive::validate_transaction(
				TransactionSource::InBlock,
				invalid.clone(),
				Default::default(),
			),
			Err(TransactionValidityError::Unknown(UnknownTransaction::NoUnsignedValidator)),
		);
		// Need to initialize the block before applying extrinsics for the `MockedSystemCallbacks`
		// check.
		Executive::initialize_block(&Header::new_from_number(1));
		assert_eq!(Executive::apply_extrinsic(valid), Ok(Err(DispatchError::BadOrigin)));
		assert_eq!(
			Executive::apply_extrinsic(invalid),
			Err(TransactionValidityError::Unknown(UnknownTransaction::NoUnsignedValidator))
		);
	});
}

#[test]
fn can_not_pay_for_tx_fee_on_full_lock() {
	let mut t = new_test_ext(1);
	t.execute_with(|| {
		<pallet_balances::Pallet<Runtime> as fungible::MutateFreeze<u64>>::set_freeze(&(), &1, 110)
			.unwrap();
		let xt = TestXt::new(
			RuntimeCall::System(frame_system::Call::remark { remark: vec![1u8] }),
			sign_extra(1, 0, 0),
		);
		Executive::initialize_block(&Header::new_from_number(1));

		assert_eq!(Executive::apply_extrinsic(xt), Err(InvalidTransaction::Payment.into()),);
		assert_eq!(<pallet_balances::Pallet<Runtime>>::total_balance(&1), 111);
	});
}

#[test]
fn block_hooks_weight_is_stored() {
	new_test_ext(1).execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));
		Executive::finalize_block();
		// NOTE: might need updates over time if new weights are introduced.
		// For now it only accounts for the base block execution weight and
		// the `on_initialize` weight defined in the custom test module.
		assert_eq!(
			<frame_system::Pallet<Runtime>>::block_weight().total(),
			Weight::from_parts(175 + 175 + 10, 0)
		);
	})
}

#[test]
fn runtime_upgraded_should_work() {
	new_test_ext(1).execute_with(|| {
		RuntimeVersionTestValues::mutate(|v| *v = Default::default());
		// It should be added at genesis
		assert!(LastRuntimeUpgrade::<Runtime>::exists());
		assert!(!Executive::runtime_upgraded());

		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion { spec_version: 1, ..Default::default() }
		});
		assert!(Executive::runtime_upgraded());

		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion {
				spec_version: 1,
				spec_name: "test".into(),
				..Default::default()
			}
		});
		assert!(Executive::runtime_upgraded());

		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion {
				spec_version: 0,
				impl_version: 2,
				..Default::default()
			}
		});
		assert!(!Executive::runtime_upgraded());

		LastRuntimeUpgrade::<Runtime>::take();
		assert!(Executive::runtime_upgraded());
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
		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion { spec_version: 1, ..Default::default() }
		});

		Executive::initialize_block(&Header::new_from_number(1));

		assert_eq!(&sp_io::storage::get(TEST_KEY).unwrap()[..], *b"module");
		assert_eq!(sp_io::storage::get(CUSTOM_ON_RUNTIME_KEY).unwrap(), true.encode());
		assert_eq!(
			Some(RuntimeVersionTestValues::get().into()),
			LastRuntimeUpgrade::<Runtime>::get(),
		)
	});
}

#[test]
fn event_from_runtime_upgrade_is_included() {
	new_test_ext(1).execute_with(|| {
		// Make sure `on_runtime_upgrade` is called.
		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion { spec_version: 1, ..Default::default() }
		});

		// set block number to non zero so events are not excluded
		System::set_block_number(1);

		Executive::initialize_block(&Header::new_from_number(2));
		System::assert_last_event(frame_system::Event::<Runtime>::CodeUpdated.into());
	});
}

/// Regression test that ensures that the custom on runtime upgrade is called when executive is
/// used through the `ExecuteBlock` trait.
#[test]
fn custom_runtime_upgrade_is_called_when_using_execute_block_trait() {
	let xt = TestXt::new(
		RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 33, value: 0 }),
		sign_extra(1, 0, 0),
	);

	let header = new_test_ext(1).execute_with(|| {
		// Make sure `on_runtime_upgrade` is called.
		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion { spec_version: 1, ..Default::default() }
		});

		// Let's build some fake block.
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	// Reset to get the correct new genesis below.
	RuntimeVersionTestValues::mutate(|v| {
		*v = sp_version::RuntimeVersion { spec_version: 0, ..Default::default() }
	});

	new_test_ext(1).execute_with(|| {
		// Make sure `on_runtime_upgrade` is called.
		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion { spec_version: 1, ..Default::default() }
		});

		<Executive as ExecuteBlock<Block<TestXt>>>::execute_block(Block::new(header, vec![xt]));

		assert_eq!(&sp_io::storage::get(TEST_KEY).unwrap()[..], *b"module");
		assert_eq!(sp_io::storage::get(CUSTOM_ON_RUNTIME_KEY).unwrap(), true.encode());
	});
}

#[test]
fn all_weights_are_recorded_correctly() {
	// Reset to get the correct new genesis below.
	RuntimeVersionTestValues::take();

	new_test_ext(1).execute_with(|| {
		// Make sure `on_runtime_upgrade` is called for maximum complexity
		RuntimeVersionTestValues::mutate(|v| {
			*v = sp_version::RuntimeVersion { spec_version: 1, ..Default::default() }
		});

		let block_number = 1;

		Executive::initialize_block(&Header::new_from_number(block_number));

		// Reset the last runtime upgrade info, to make the second call to `on_runtime_upgrade`
		// succeed.
		LastRuntimeUpgrade::<Runtime>::take();
		MockedSystemCallbacks::reset();

		// All weights that show up in the `initialize_block_impl`
		let custom_runtime_upgrade_weight = CustomOnRuntimeUpgrade::on_runtime_upgrade();
		let runtime_upgrade_weight =
			<AllPalletsWithSystem as OnRuntimeUpgrade>::on_runtime_upgrade();
		let on_initialize_weight =
			<AllPalletsWithSystem as OnInitialize<u64>>::on_initialize(block_number);
		let base_block_weight = <Runtime as frame_system::Config>::BlockWeights::get().base_block;

		// Weights are recorded correctly
		assert_eq!(
			frame_system::Pallet::<Runtime>::block_weight().total(),
			custom_runtime_upgrade_weight +
				runtime_upgrade_weight +
				on_initialize_weight +
				base_block_weight,
		);
	});
}

#[test]
fn offchain_worker_works_as_expected() {
	new_test_ext(1).execute_with(|| {
		let parent_hash = sp_core::H256::from([69u8; 32]);
		let mut digest = Digest::default();
		digest.push(DigestItem::Seal([1, 2, 3, 4], vec![5, 6, 7, 8]));

		let header = Header::new(1, H256::default(), H256::default(), parent_hash, digest.clone());

		Executive::offchain_worker(&header);

		assert_eq!(digest, System::digest());
		assert_eq!(parent_hash, System::block_hash(0));
		assert_eq!(header.hash(), System::block_hash(1));
	});
}

#[test]
fn calculating_storage_root_twice_works() {
	let call = RuntimeCall::Custom(custom::Call::calculate_storage_root {});
	let xt = TestXt::new(call, sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		// Let's build some fake block.
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		Executive::execute_block(Block::new(header, vec![xt]));
	});
}

#[test]
#[should_panic(expected = "Invalid inherent position for extrinsic at index 1")]
fn invalid_inherent_position_fail() {
	let xt1 = TestXt::new(
		RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 33, value: 0 }),
		sign_extra(1, 0, 0),
	);
	let xt2 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);

	let header = new_test_ext(1).execute_with(|| {
		// Let's build some fake block.
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt2.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		Executive::execute_block(Block::new(header, vec![xt1, xt2]));
	});
}

#[test]
fn valid_inherents_position_works() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);
	let xt2 = TestXt::new(call_transfer(33, 0), sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		// Let's build some fake block.
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt2.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		Executive::execute_block(Block::new(header, vec![xt1, xt2]));
	});
}

#[test]
#[should_panic(expected = "A call was labelled as mandatory, but resulted in an Error.")]
fn invalid_inherents_fail_block_execution() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), sign_extra(1, 0, 0));

	new_test_ext(1).execute_with(|| {
		Executive::execute_block(Block::new(
			Header::new(1, H256::default(), H256::default(), [69u8; 32].into(), Digest::default()),
			vec![xt1],
		));
	});
}

// Inherents are created by the runtime and don't need to be validated.
#[test]
fn inherents_fail_validate_block() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);

	new_test_ext(1).execute_with(|| {
		assert_eq!(
			Executive::validate_transaction(TransactionSource::External, xt1, H256::random())
				.unwrap_err(),
			InvalidTransaction::MandatoryValidation.into()
		);
	})
}

/// Inherents still work while `initialize_block` forbids transactions.
#[test]
fn inherents_ok_while_exts_forbidden_works() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);

	let header = new_test_ext(1).execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();
		// This is not applied:
		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		// Tell `initialize_block` to forbid extrinsics:
		Executive::execute_block(Block::new(header, vec![xt1]));
	});
}

/// Refuses to import blocks with transactions during `OnlyInherents` mode.
#[test]
#[should_panic = "Only inherents are allowed in this block"]
fn transactions_in_only_inherents_block_errors() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);
	let xt2 = TestXt::new(call_transfer(33, 0), sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt2.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		MbmActive::set(true);
		Executive::execute_block(Block::new(header, vec![xt1, xt2]));
	});
}

/// Same as above but no error.
#[test]
fn transactions_in_normal_block_works() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);
	let xt2 = TestXt::new(call_transfer(33, 0), sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt2.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		// Tell `initialize_block` to forbid extrinsics:
		Executive::execute_block(Block::new(header, vec![xt1, xt2]));
	});
}

#[test]
#[cfg(feature = "try-runtime")]
fn try_execute_block_works() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);
	let xt2 = TestXt::new(call_transfer(33, 0), sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt2.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		Executive::try_execute_block(
			Block::new(header, vec![xt1, xt2]),
			true,
			true,
			frame_try_runtime::TryStateSelect::All,
		)
		.unwrap();
	});
}

/// Same as `extrinsic_while_exts_forbidden_errors` but using the try-runtime function.
#[test]
#[cfg(feature = "try-runtime")]
#[should_panic = "Only inherents allowed"]
fn try_execute_tx_forbidden_errors() {
	let xt1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);
	let xt2 = TestXt::new(call_transfer(33, 0), sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		// Let's build some fake block.
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt2.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		MbmActive::set(true);
		Executive::try_execute_block(
			Block::new(header, vec![xt1, xt2]),
			true,
			true,
			frame_try_runtime::TryStateSelect::All,
		)
		.unwrap();
	});
}

/// Check that `ensure_inherents_are_first` reports the correct indices.
#[test]
fn ensure_inherents_are_first_works() {
	let in1 = TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None);
	let in2 = TestXt::new(RuntimeCall::Custom2(custom2::Call::inherent {}), None);
	let xt2 = TestXt::new(call_transfer(33, 0), sign_extra(1, 0, 0));

	// Mocked empty header:
	let header = new_test_ext(1).execute_with(|| {
		Executive::initialize_block(&Header::new_from_number(1));
		Executive::finalize_block()
	});

	new_test_ext(1).execute_with(|| {
		assert_ok!(Runtime::ensure_inherents_are_first(&Block::new(header.clone(), vec![]),), 0);
		assert_ok!(
			Runtime::ensure_inherents_are_first(&Block::new(header.clone(), vec![xt2.clone()]),),
			0
		);
		assert_ok!(
			Runtime::ensure_inherents_are_first(&Block::new(header.clone(), vec![in1.clone()])),
			1
		);
		assert_ok!(
			Runtime::ensure_inherents_are_first(&Block::new(
				header.clone(),
				vec![in1.clone(), xt2.clone()]
			),),
			1
		);
		assert_ok!(
			Runtime::ensure_inherents_are_first(&Block::new(
				header.clone(),
				vec![in2.clone(), in1.clone(), xt2.clone()]
			),),
			2
		);

		assert_eq!(
			Runtime::ensure_inherents_are_first(&Block::new(
				header.clone(),
				vec![xt2.clone(), in1.clone()]
			),),
			Err(1)
		);
		assert_eq!(
			Runtime::ensure_inherents_are_first(&Block::new(
				header.clone(),
				vec![xt2.clone(), xt2.clone(), in1.clone()]
			),),
			Err(2)
		);
		assert_eq!(
			Runtime::ensure_inherents_are_first(&Block::new(
				header.clone(),
				vec![xt2.clone(), xt2.clone(), xt2.clone(), in2.clone()]
			),),
			Err(3)
		);
	});
}

/// Check that block execution rejects blocks with transactions in them while MBMs are active and
/// also that all the system callbacks are called correctly.
#[test]
fn callbacks_in_block_execution_works() {
	callbacks_in_block_execution_works_inner(false);
	callbacks_in_block_execution_works_inner(true);
}

fn callbacks_in_block_execution_works_inner(mbms_active: bool) {
	MbmActive::set(mbms_active);

	for (n_in, n_tx) in (0..10usize).zip(0..10usize) {
		let mut extrinsics = Vec::new();

		let header = new_test_ext(10).execute_with(|| {
			MockedSystemCallbacks::reset();
			Executive::initialize_block(&Header::new_from_number(1));
			assert_eq!(SystemCallbacksCalled::get(), 1);

			for i in 0..n_in {
				let xt = if i % 2 == 0 {
					TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None)
				} else {
					TestXt::new(RuntimeCall::Custom2(custom2::Call::optional_inherent {}), None)
				};
				Executive::apply_extrinsic(xt.clone()).unwrap().unwrap();
				extrinsics.push(xt);
			}

			for t in 0..n_tx {
				let xt = TestXt::new(
					RuntimeCall::Custom2(custom2::Call::some_call {}),
					sign_extra(1, t as u64, 0),
				);
				// Extrinsics can be applied even when MBMs are active. Only the `execute_block`
				// will reject it.
				Executive::apply_extrinsic(xt.clone()).unwrap().unwrap();
				extrinsics.push(xt);
			}

			Executive::finalize_block()
		});
		assert_eq!(SystemCallbacksCalled::get(), 3);

		new_test_ext(10).execute_with(|| {
			let header = std::panic::catch_unwind(|| {
				Executive::execute_block(Block::new(header, extrinsics));
			});

			match header {
				Err(e) => {
					let err = e.downcast::<&str>().unwrap();
					assert_eq!(*err, "Only inherents are allowed in this block");
					assert!(
						MbmActive::get() && n_tx > 0,
						"Transactions should be rejected when MBMs are active"
					);
				},
				Ok(_) => {
					assert_eq!(SystemCallbacksCalled::get(), 3);
					assert!(
						!MbmActive::get() || n_tx == 0,
						"MBMs should be deactivated after finalization"
					);
				},
			}
		});
	}
}

#[test]
fn post_inherent_called_after_all_inherents() {
	let in1 = TestXt::new(RuntimeCall::Custom2(custom2::Call::inherent {}), None);
	let xt1 = TestXt::new(RuntimeCall::Custom2(custom2::Call::some_call {}), sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		// Let's build some fake block.
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(in1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	#[cfg(feature = "try-runtime")]
	new_test_ext(1).execute_with(|| {
		Executive::try_execute_block(
			Block::new(header.clone(), vec![in1.clone(), xt1.clone()]),
			true,
			true,
			frame_try_runtime::TryStateSelect::All,
		)
		.unwrap();
		assert!(MockedSystemCallbacks::post_transactions_called());
	});

	new_test_ext(1).execute_with(|| {
		MockedSystemCallbacks::reset();
		Executive::execute_block(Block::new(header, vec![in1, xt1]));
		assert!(MockedSystemCallbacks::post_transactions_called());
	});
}

/// Regression test for AppSec finding #40.
#[test]
fn post_inherent_called_after_all_optional_inherents() {
	let in1 = TestXt::new(RuntimeCall::Custom2(custom2::Call::optional_inherent {}), None);
	let xt1 = TestXt::new(RuntimeCall::Custom2(custom2::Call::some_call {}), sign_extra(1, 0, 0));

	let header = new_test_ext(1).execute_with(|| {
		// Let's build some fake block.
		Executive::initialize_block(&Header::new_from_number(1));

		Executive::apply_extrinsic(in1.clone()).unwrap().unwrap();
		Executive::apply_extrinsic(xt1.clone()).unwrap().unwrap();

		Executive::finalize_block()
	});

	#[cfg(feature = "try-runtime")]
	new_test_ext(1).execute_with(|| {
		Executive::try_execute_block(
			Block::new(header.clone(), vec![in1.clone(), xt1.clone()]),
			true,
			true,
			frame_try_runtime::TryStateSelect::All,
		)
		.unwrap();
		assert!(MockedSystemCallbacks::post_transactions_called());
	});

	new_test_ext(1).execute_with(|| {
		MockedSystemCallbacks::reset();
		Executive::execute_block(Block::new(header, vec![in1, xt1]));
		assert!(MockedSystemCallbacks::post_transactions_called());
	});
}

#[test]
fn is_inherent_works() {
	let ext = TestXt::new(RuntimeCall::Custom2(custom2::Call::inherent {}), None);
	assert!(Runtime::is_inherent(&ext));
	let ext = TestXt::new(RuntimeCall::Custom2(custom2::Call::optional_inherent {}), None);
	assert!(Runtime::is_inherent(&ext));

	let ext = TestXt::new(call_transfer(33, 0), sign_extra(1, 0, 0));
	assert!(!Runtime::is_inherent(&ext));

	let ext = TestXt::new(RuntimeCall::Custom2(custom2::Call::allowed_unsigned {}), None);
	assert!(!Runtime::is_inherent(&ext), "Unsigned ext are not automatically inherents");
}
