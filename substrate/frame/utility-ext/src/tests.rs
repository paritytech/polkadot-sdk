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

// Tests for the Utility Extension Pallet

#![cfg(test)]

use super::*;

use crate as pallet_utility_ext;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU8, Everything, Get},
	weights::{IdentityFee, Weight},
};
use frame_system::EnsureRoot;
use pallet_collective::{EnsureProportionAtLeast, Instance1};
use pallet_transaction_payment::{ChargeTransactionPayment, FungibleAdapter};
use sp_runtime::BuildStorage;

type BlockNumber = u64;

#[frame_support::pallet(dev_mode)]
pub mod example {
	use frame_support::{dispatch::WithPostDispatchInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type CouncilOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// What the fee payment extension's transient credit holds.
		type FeeCredit: Get<u64>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(*_weight)]
		pub fn noop(_origin: OriginFor<T>, _weight: Weight) -> DispatchResult {
			Ok(())
		}

		/// Authorized while nothing was claimed yet. Like some in-tree calls, dispatch relies on
		/// `authorize` having just run and does not check again.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::zero())]
		#[pallet::weight_of_authorize(Weight::zero())]
		#[pallet::authorize(Self::authorize_claim)]
		pub fn claim(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized(origin)?;
			Claims::<T>::mutate(|n| *n += 1);
			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight((Weight::from_parts(10, 0), Pays::No))]
		pub fn free(origin: OriginFor<T>, err: bool) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!err, Error::<T>::Custom);
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(*_start_weight)]
		pub fn foobar(
			origin: OriginFor<T>,
			err: bool,
			_start_weight: Weight,
			end_weight: Option<Weight>,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			Calls::<T>::mutate(|n| *n += 1);
			if err {
				let error: DispatchError = Error::<T>::Custom.into();
				if let Some(weight) = end_weight {
					Err(error.with_weight(weight))
				} else {
					Err(error)?
				}
			} else {
				Ok(end_weight.into())
			}
		}

		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn council_only(origin: OriginFor<T>) -> DispatchResult {
			T::CouncilOrigin::ensure_origin(origin)?;
			CouncilCalls::<T>::mutate(|n| *n += 1);
			Ok(())
		}

		/// Records the withdrawn fee it can see.
		#[pallet::call_index(5)]
		#[pallet::weight(0)]
		pub fn observe_fee_credit(origin: OriginFor<T>) -> DispatchResult {
			ensure_signed(origin)?;
			ObservedFeeCredit::<T>::put(T::FeeCredit::get());
			Ok(())
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		Custom,
	}

	/// What the last `observe_fee_credit` saw.
	#[pallet::storage]
	pub type ObservedFeeCredit<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// How many times `foobar` was dispatched successfully.
	#[pallet::storage]
	pub type Calls<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// How many times `council_only` was dispatched.
	#[pallet::storage]
	pub type CouncilCalls<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// How many times `claim` was dispatched.
	#[pallet::storage]
	pub type Claims<T: Config> = StorageValue<_, u32, ValueQuery>;

	impl<T: Config> Pallet<T> {
		pub fn authorize_claim(_source: TransactionSource) -> TransactionValidityWithRefund {
			if Claims::<T>::get() > 0 {
				return Err(InvalidTransaction::Stale.into());
			}
			let validity =
				ValidTransaction { provides: vec![b"claim".to_vec()], ..Default::default() };
			Ok((validity, Weight::zero()))
		}
	}
}

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		TransactionPayment: pallet_transaction_payment,
		Council: pallet_collective::<Instance1>,
		Utility: pallet_utility,
		UtilityExt: pallet_utility_ext,
		Example: example,
	}
);

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(Weight::MAX);
}
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = BlockWeights;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

/// Fees are the transaction's length plus its weight's `ref_time`.
#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Test {
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<u64>;
	type LengthToFee = IdentityFee<u64>;
}

parameter_types! {
	pub const MotionDuration: BlockNumber = 3;
	pub const MaxProposals: u32 = 100;
	pub const MaxMembers: u32 = 100;
	pub MaxProposalWeight: Weight =
		sp_runtime::Perbill::from_percent(50) * BlockWeights::get().max_block;
}

type CouncilCollective = pallet_collective::Instance1;
impl pallet_collective::Config<CouncilCollective> for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDuration;
	type MaxProposals = MaxProposals;
	type MaxMembers = MaxMembers;
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	type WeightInfo = ();
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
}

impl example::Config for Test {
	type CouncilOrigin = EnsureProportionAtLeast<u64, Instance1, 3, 4>;
	type FeeCredit = FeeCredit;
}

pub struct FeeCredit;
impl Get<u64> for FeeCredit {
	fn get() -> u64 {
		TransactionPayment::remaining_txfee::<u64>()
	}
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

impl Config for Test {
	type InnerExtension = InnerExtension;
	type FeeCredit = TransactionPayment;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchmarkItems;
	type MaxMultiOriginBatch = ConstU32<4>;
	type MultiOriginCallFilter = MultiOriginCallFilter;
	type WeightInfo = ();
}

/// Keeps `Example::noop` out of multi-origin batches.
pub struct MultiOriginCallFilter;
impl Contains<RuntimeCall> for MultiOriginCallFilter {
	fn contains(c: &RuntimeCall) -> bool {
		!matches!(c, RuntimeCall::Example(ExampleCall::noop { .. }))
	}
}

/// The inner extension pipeline of multi-origin batch items in tests.
pub type InnerExtension = (
	ExpectImplication,
	SetOrigin,
	frame_system::AuthorizeCall<Test>,
	frame_system::CheckNonce<Test>,
	ChargeTransactionPayment<Test>,
);

/// Builds the benchmark items: signed by account 1.
#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkItems;
#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper<Test> for BenchmarkItems {
	fn setup() -> OriginCaller {
		OriginCaller::system(frame_system::RawOrigin::Signed(1))
	}

	fn item(call: RuntimeCall, nonce: u32, implication: Vec<u8>) -> MultiOriginItemOf<Test> {
		MultiOriginItem::new(
			call,
			(
				ExpectImplication(implication),
				SetOrigin::signed(1),
				frame_system::AuthorizeCall::new(),
				frame_system::CheckNonce::from(nonce),
				ChargeTransactionPayment::from(0),
			),
			Self::setup(),
		)
	}
}

use multi_origin_mock::*;

/// Mock pieces for the multi-origin batch tests.
mod multi_origin_mock {
	use super::*;
	use frame_support::{pallet_prelude::TransactionSource, storage};
	use sp_runtime::{
		traits::{Implication, TransactionExtension, ValidateResult},
		transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
		DispatchResult,
	};

	/// The runtime's transaction extension pipeline in tests.
	pub type TxExtension = (
		frame_system::CheckNonce<Test>,
		frame_system::CheckWeight<Test>,
		MultiOriginBatch<Test, ChargeTransactionPayment<Test>>,
	);
	pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
		u64,
		RuntimeCall,
		sp_runtime::testing::TestSignature,
		TxExtension,
	>;

	/// Keys of the counters `SetOrigin` keeps in storage.
	pub const SET_ORIGIN_PREPARED: &[u8] = b"SetOrigin::prepared";
	pub const SET_ORIGIN_POST_DISPATCHED: &[u8] = b"SetOrigin::post_dispatched";
	/// Set to make `SetOrigin::post_dispatch` fail.
	pub const SET_ORIGIN_FAIL_POST_DISPATCH: &[u8] = b"SetOrigin::fail_post_dispatch";
	/// Set to make `SetOrigin::post_dispatch` refund its whole weight.
	pub const SET_ORIGIN_REFUND: &[u8] = b"SetOrigin::refund";
	pub const SET_ORIGIN_POST_DISPATCH_ERROR: InvalidTransaction = InvalidTransaction::Custom(7);
	/// The weight `SetOrigin` declares.
	pub const SET_ORIGIN_WEIGHT: Weight = Weight::from_parts(7, 0);

	/// A test inner extension which stands in for a signature check: it fails with `BadProof`
	/// unless the base implication it is validated with is the one it carries.
	#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo, Debug)]
	pub struct ExpectImplication(pub Vec<u8>);

	impl TransactionExtension<RuntimeCall> for ExpectImplication {
		const IDENTIFIER: &'static str = "ExpectImplication";
		type Implicit = ();
		type Val = ();
		type Pre = ();

		fn weight(&self, _: &RuntimeCall) -> Weight {
			Weight::zero()
		}

		fn validate(
			&self,
			origin: RuntimeOrigin,
			_: &RuntimeCall,
			_: &DispatchInfo,
			_: usize,
			_: (),
			inherited_implication: &impl Implication,
			_: TransactionSource,
		) -> ValidateResult<(), RuntimeCall> {
			if inherited_implication.parts().base.encode() != self.0 {
				return Err(InvalidTransaction::BadProof.into());
			}
			Ok((ValidTransaction::default(), (), origin))
		}

		sp_runtime::impl_tx_ext_default!(RuntimeCall; prepare);
	}

	/// A test inner extension which authorizes the origin it carries and counts how often its
	/// `prepare` and `post_dispatch` run.
	#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo, Debug)]
	pub struct SetOrigin(pub OriginCaller);

	impl SetOrigin {
		pub fn signed(who: u64) -> Self {
			Self(OriginCaller::system(frame_system::RawOrigin::Signed(who)))
		}
		pub fn none() -> Self {
			Self(OriginCaller::system(frame_system::RawOrigin::None))
		}
		pub fn council() -> Self {
			Self(OriginCaller::Council(pallet_collective::RawOrigin::Members(3, 3)))
		}
		pub fn prepared() -> u32 {
			storage::unhashed::get_or_default(SET_ORIGIN_PREPARED)
		}
		pub fn post_dispatched() -> u32 {
			storage::unhashed::get_or_default(SET_ORIGIN_POST_DISPATCHED)
		}
		pub fn fail_post_dispatch() {
			storage::unhashed::put(SET_ORIGIN_FAIL_POST_DISPATCH, &true);
		}
		pub fn refund() {
			storage::unhashed::put(SET_ORIGIN_REFUND, &true);
		}
		fn bump(key: &[u8]) {
			storage::unhashed::put(key, &(storage::unhashed::get_or_default::<u32>(key) + 1));
		}
	}

	impl TransactionExtension<RuntimeCall> for SetOrigin {
		const IDENTIFIER: &'static str = "SetOrigin";
		type Implicit = ();
		type Val = ();
		type Pre = ();

		fn weight(&self, _: &RuntimeCall) -> Weight {
			SET_ORIGIN_WEIGHT
		}

		fn validate(
			&self,
			mut origin: RuntimeOrigin,
			_: &RuntimeCall,
			_: &DispatchInfo,
			_: usize,
			_: (),
			_: &impl Implication,
			_: TransactionSource,
		) -> ValidateResult<(), RuntimeCall> {
			origin.set_caller(self.0.clone());
			Ok((ValidTransaction::default(), (), origin))
		}

		fn prepare(
			self,
			_: (),
			_: &RuntimeOrigin,
			_: &RuntimeCall,
			_: &DispatchInfo,
			_: usize,
		) -> Result<(), TransactionValidityError> {
			Self::bump(SET_ORIGIN_PREPARED);
			Ok(())
		}

		fn post_dispatch_details(
			_: (),
			_: &DispatchInfo,
			_: &PostDispatchInfo,
			_: usize,
			_: &DispatchResult,
		) -> Result<Weight, TransactionValidityError> {
			Self::bump(SET_ORIGIN_POST_DISPATCHED);
			if storage::unhashed::get_or_default::<bool>(SET_ORIGIN_FAIL_POST_DISPATCH) {
				return Err(SET_ORIGIN_POST_DISPATCH_ERROR.into());
			}
			if storage::unhashed::get_or_default::<bool>(SET_ORIGIN_REFUND) {
				return Ok(SET_ORIGIN_WEIGHT);
			}
			Ok(Weight::zero())
		}
	}
}

type ExampleCall = example::Call<Test>;

pub const INITIAL_BALANCE: u64 = 1_000_000_000_000;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: (1..=5).map(|who| (who, INITIAL_BALANCE)).collect(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_collective::GenesisConfig::<Test, Instance1> {
		members: vec![1, 2, 3],
		phantom: Default::default(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

mod multi_origin {
	use super::*;
	use frame_support::{
		assert_noop, assert_ok,
		dispatch::{DispatchErrorWithPostInfo, GetDispatchInfo, Pays},
		pallet_prelude::TransactionSource,
		storage::{with_transaction, TransactionOutcome},
	};
	use frame_system::Call as SystemCall;
	use sp_runtime::{
		generic::SignedPayload,
		testing::TestSignature,
		traits::{Applyable, BadOrigin, Checkable, DispatchTransaction, IdentityLookup},
		transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
		ApplyExtrinsicResultWithInfo, DispatchError,
	};

	type BalancesCall = pallet_balances::Call<Test>;
	type UtilityCall = pallet_utility::Call<Test>;
	type UtilityExtCall = crate::Call<Test>;

	/// The spec of an item: what it dispatches, who authorizes it and what it declares.
	#[derive(Clone)]
	struct ItemSpec {
		call: RuntimeCall,
		set_origin: SetOrigin,
		/// The origin the item states; by default the one `set_origin` authorizes.
		expected: OriginCaller,
		nonce: u32,
	}

	/// An item dispatching `call` with the origin `set_origin` authorizes, with the current
	/// nonce of that origin if it is signed.
	fn spec(call: RuntimeCall, set_origin: SetOrigin) -> ItemSpec {
		let nonce = match &set_origin.0 {
			OriginCaller::system(frame_system::RawOrigin::Signed(who)) => {
				System::account_nonce(who)
			},
			_ => 0,
		};
		ItemSpec { call, expected: set_origin.0.clone(), set_origin, nonce }
	}

	fn with_nonce(mut spec: ItemSpec, nonce: u32) -> ItemSpec {
		spec.nonce = nonce;
		spec
	}

	fn expecting(mut spec: ItemSpec, origin: OriginCaller) -> ItemSpec {
		spec.expected = origin;
		spec
	}

	fn signed_by(who: u64) -> OriginCaller {
		OriginCaller::system(frame_system::RawOrigin::Signed(who))
	}

	fn authorized_origin() -> OriginCaller {
		OriginCaller::system(frame_system::RawOrigin::Authorized)
	}

	/// An item authorized by `AuthorizeCall`: nothing sets an origin before it.
	fn authorized(call: RuntimeCall) -> ItemSpec {
		expecting(spec(call, SetOrigin::none()), authorized_origin())
	}

	fn call_transfer(dest: u64, value: u64) -> RuntimeCall {
		RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value })
	}

	fn call_foobar(err: bool, start: u64, end: Option<u64>) -> RuntimeCall {
		RuntimeCall::Example(ExampleCall::foobar {
			err,
			start_weight: Weight::from_parts(start, 0),
			end_weight: end.map(|w| Weight::from_parts(w, 0)),
		})
	}

	fn call_free(err: bool) -> RuntimeCall {
		RuntimeCall::Example(ExampleCall::free { err })
	}

	fn call_observe() -> RuntimeCall {
		RuntimeCall::Example(ExampleCall::observe_fee_credit {})
	}

	fn call_claim() -> RuntimeCall {
		RuntimeCall::Example(ExampleCall::claim {})
	}

	fn call_remark() -> RuntimeCall {
		RuntimeCall::System(SystemCall::remark { remark: vec![] })
	}

	/// The items of a batch in `mode` submitted by `submitter` with `nonce`, each "signed" over
	/// the implication of its batch.
	fn build_items(
		submitter: u64,
		nonce: u32,
		mode: BatchMode,
		specs: &[ItemSpec],
	) -> Vec<MultiOriginItemOf<Test>> {
		let items_hash =
			Pallet::<Test>::items_hash(specs.iter().map(|spec| (&spec.call, &spec.expected)));
		specs
			.iter()
			.enumerate()
			.map(|(index, spec)| {
				let implication = ItemImplication {
					mode,
					submitter: &submitter,
					nonce,
					index: index as u32,
					items_hash: &items_hash,
				}
				.encode();
				MultiOriginItem::new(
					spec.call.clone(),
					(
						ExpectImplication(implication),
						spec.set_origin.clone(),
						frame_system::AuthorizeCall::new(),
						frame_system::CheckNonce::from(spec.nonce),
						ChargeTransactionPayment::from(0),
					),
					spec.expected.clone(),
				)
			})
			.collect()
	}

	/// The batch call of `items` in `mode` carrying `nonce`.
	fn batch_call(mode: BatchMode, nonce: u32, items: Vec<MultiOriginItemOf<Test>>) -> RuntimeCall {
		let items = items.try_into().expect("at most `MaxMultiOriginBatch` items");
		match mode {
			BatchMode::StopAtFailure => {
				RuntimeCall::UtilityExt(UtilityExtCall::batch_multi_origin { items, nonce })
			},
			BatchMode::All => {
				RuntimeCall::UtilityExt(UtilityExtCall::force_batch_multi_origin { items, nonce })
			},
		}
	}

	/// The items of the batch call `call`.
	fn items_of(call: &RuntimeCall) -> &MultiOriginItemsOf<Test> {
		match call {
			RuntimeCall::UtilityExt(
				UtilityExtCall::batch_multi_origin { items, .. } |
				UtilityExtCall::force_batch_multi_origin { items, .. },
			) => items,
			_ => panic!("not a batch"),
		}
	}

	/// The runtime's pipeline without [`MultiOriginBatch`].
	type PlainExtension = (
		frame_system::CheckNonce<Test>,
		frame_system::CheckWeight<Test>,
		ChargeTransactionPayment<Test>,
	);
	type PlainExtrinsic =
		sp_runtime::generic::UncheckedExtrinsic<u64, RuntimeCall, TestSignature, PlainExtension>;

	/// Apply `call` signed by `who` with their current nonce through the plain pipeline. Returns
	/// what the call returned, the transaction's length and what `who` paid.
	fn apply_plain(who: u64, call: RuntimeCall) -> (PostDispatchInfo, usize, u64) {
		let ext: PlainExtension = (
			frame_system::CheckNonce::from(System::account_nonce(who)),
			frame_system::CheckWeight::new(),
			ChargeTransactionPayment::from(0),
		);
		let payload = SignedPayload::new(call.clone(), ext.clone())
			.expect("implicit data is available")
			.using_encoded(|payload| payload.to_vec());
		let uxt = PlainExtrinsic::new_signed(call, who, TestSignature(who, payload), ext);
		let info = uxt.get_dispatch_info();
		let len = uxt.encoded_size();
		let xt =
			<PlainExtrinsic as Checkable<IdentityLookup<u64>>>::check(uxt, &Default::default())
				.expect("test signatures check");
		let post_info = xt.apply::<Test>(&info, len).unwrap().unwrap();
		(post_info, len, fee_paid_by(who))
	}

	fn tx_ext(nonce: u32, tip: u64) -> TxExtension {
		(
			frame_system::CheckNonce::from(nonce),
			frame_system::CheckWeight::new(),
			MultiOriginBatch::from(ChargeTransactionPayment::from(tip)),
		)
	}

	/// A transaction of `call` signed by `who` with their current nonce.
	fn signed_tx(who: u64, call: RuntimeCall) -> UncheckedExtrinsic {
		tipped_tx(who, call, 0)
	}

	fn tipped_tx(who: u64, call: RuntimeCall, tip: u64) -> UncheckedExtrinsic {
		let ext = tx_ext(System::account_nonce(who), tip);
		let payload = SignedPayload::new(call.clone(), ext.clone())
			.expect("implicit data is available")
			.using_encoded(|payload| payload.to_vec());
		UncheckedExtrinsic::new_signed(call, who, TestSignature(who, payload), ext)
	}

	/// A batch of `specs` in `mode`, submitted by `submitter` with their current nonce.
	fn batch_tx(submitter: u64, mode: BatchMode, specs: &[ItemSpec]) -> UncheckedExtrinsic {
		let nonce = System::account_nonce(submitter);
		signed_tx(submitter, batch_call(mode, nonce, build_items(submitter, nonce, mode, specs)))
	}

	fn check(
		uxt: UncheckedExtrinsic,
	) -> (<UncheckedExtrinsic as Checkable<IdentityLookup<u64>>>::Checked, DispatchInfo, usize) {
		let info = uxt.get_dispatch_info();
		let len = uxt.encoded_size();
		let xt =
			<UncheckedExtrinsic as Checkable<IdentityLookup<u64>>>::check(uxt, &Default::default())
				.expect("test signatures check");
		(xt, info, len)
	}

	fn validate(uxt: UncheckedExtrinsic) -> Result<ValidTransaction, TransactionValidityError> {
		let (xt, info, len) = check(uxt);
		xt.validate::<Test>(TransactionSource::External, &info, len)
	}

	fn apply(uxt: UncheckedExtrinsic) -> ApplyExtrinsicResultWithInfo<PostDispatchInfo> {
		let (xt, info, len) = check(uxt);
		xt.apply::<Test>(&info, len)
	}

	/// Apply a batch and assert the transaction itself went through.
	fn apply_ok(uxt: UncheckedExtrinsic) -> PostDispatchInfo {
		apply(uxt).expect("the transaction is valid").expect("the batch call succeeds")
	}

	/// The fee for `weight` and `len`, the only two things fees depend on in this mock.
	fn fee(weight: Weight, len: usize) -> u64 {
		weight.ref_time() + len as u64
	}

	/// The fee for the declared weight and length of `uxt`.
	fn full_fee(uxt: &UncheckedExtrinsic) -> u64 {
		fee(uxt.get_dispatch_info().total_weight(), uxt.encoded_size())
	}

	/// What `who` paid for their last transaction.
	fn fee_paid_by(who: u64) -> u64 {
		System::events()
			.into_iter()
			.rev()
			.find_map(|record| match record.event {
				RuntimeEvent::TransactionPayment(
					pallet_transaction_payment::Event::TransactionFeePaid {
						who: payer,
						actual_fee,
						..
					},
				) if payer == who => Some(actual_fee),
				_ => None,
			})
			.unwrap_or_default()
	}

	/// What `who` paid for all their transactions and items.
	fn fees_paid_by(who: u64) -> u64 {
		System::events()
			.into_iter()
			.filter_map(|record| match record.event {
				RuntimeEvent::TransactionPayment(
					pallet_transaction_payment::Event::TransactionFeePaid {
						who: payer,
						actual_fee,
						..
					},
				) if payer == who => Some(actual_fee),
				_ => None,
			})
			.sum()
	}

	/// The batch overhead of `uxt`, for its number of items and their bytes.
	fn overhead(uxt: &UncheckedExtrinsic) -> Weight {
		let items = items_of(&uxt.function);
		Pallet::<Test>::overhead_weight(items.len() as u32, items.encoded_size() as u32)
	}

	/// The weight of the pipelines of the items at `indices` of the batch `uxt`.
	fn pipelines_weight(uxt: &UncheckedExtrinsic, indices: &[usize]) -> Weight {
		let items = items_of(&uxt.function);
		indices.iter().fold(Weight::zero(), |weight, index| {
			weight.saturating_add(items[*index].extension.weight(&items[*index].call))
		})
	}

	/// The bytes of the items at `indices` of the batch `uxt`.
	fn items_len(uxt: &UncheckedExtrinsic, indices: &[usize]) -> usize {
		let items = items_of(&uxt.function);
		indices.iter().map(|index| items[*index].encoded_size()).sum()
	}

	/// What the submitter of the batch `uxt` pays when every item is refunded.
	fn submitter_fee(uxt: &UncheckedExtrinsic) -> u64 {
		let all = (0..items_of(&uxt.function).len()).collect::<Vec<_>>();
		submitter_fee_with(uxt, Weight::zero(), &all)
	}

	/// What the submitter of the batch `uxt` pays: the overhead, the outer pipeline, the
	/// `unrefunded` weight and the bytes other than those of the `refunded` items.
	fn submitter_fee_with(uxt: &UncheckedExtrinsic, unrefunded: Weight, refunded: &[usize]) -> u64 {
		fee(
			overhead(uxt) + outer_weight(uxt) + unrefunded,
			uxt.encoded_size() - items_len(uxt, refunded),
		)
	}

	/// The outer pipeline's weight.
	fn outer_weight(uxt: &UncheckedExtrinsic) -> Weight {
		uxt.get_dispatch_info().extension_weight
	}

	fn w(ref_time: u64) -> Weight {
		Weight::from_parts(ref_time, 0)
	}

	fn balance(who: u64) -> u64 {
		Balances::free_balance(who)
	}

	fn pallet_events() -> Vec<Event> {
		System::events()
			.into_iter()
			.filter_map(|record| match record.event {
				RuntimeEvent::UtilityExt(event) => Some(event),
				_ => None,
			})
			.collect()
	}

	fn invalid(error: InvalidTransaction) -> ItemError {
		ItemError::Invalid(error.into())
	}

	/// Dispatch errors come back from events, which store them stripped of their messages.
	fn failed(error: impl Into<DispatchError>) -> ItemError {
		ItemError::Failed(error.into().stripped())
	}

	fn custom() -> ItemError {
		failed(example::Error::<Test>::Custom)
	}

	fn assert_fee_credit_empty() {
		assert_eq!(TransactionPayment::remaining_txfee::<u64>(), 0);
		assert!(RefundableShare::<Test>::get().is_none());
	}

	#[test]
	fn dispatches_each_item_with_its_own_origin() {
		new_test_ext().execute_with(|| {
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				spec(call_transfer(4, 200), SetOrigin::signed(2)),
				spec(RuntimeCall::Example(ExampleCall::council_only {}), SetOrigin::council()),
			];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let submitter = submitter_fee(&uxt);
			apply_ok(uxt);

			assert_eq!(balance(4), INITIAL_BALANCE + 300);
			assert!(fee_paid_by(1) > 0);
			assert_eq!(balance(1), INITIAL_BALANCE - 100 - fee_paid_by(1));
			assert_eq!(balance(2), INITIAL_BALANCE - 200 - fee_paid_by(2));
			assert!(submitter > 0);
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(balance(3), INITIAL_BALANCE - submitter);
			assert_eq!(example::CouncilCalls::<Test>::get(), 1);
			assert_eq!(System::account_nonce(1), 1);
			assert_eq!(System::account_nonce(2), 1);
			assert_eq!(System::account_nonce(3), 1);
			assert_eq!(SetOrigin::prepared(), 3);
			assert_eq!(SetOrigin::post_dispatched(), 3);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemCompleted { index: 0 },
					Event::ItemCompleted { index: 1 },
					Event::ItemCompleted { index: 2 },
					Event::BatchCompleted,
				]
			);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn items_of_one_signer_apply_in_sequence() {
		new_test_ext().execute_with(|| {
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(1)), 1),
			];
			apply_ok(batch_tx(3, BatchMode::StopAtFailure, &specs));

			assert_eq!(balance(4), INITIAL_BALANCE + 200);
			assert_eq!(System::account_nonce(1), 2);
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));

			// Two items with the same nonce: the second is stale.
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(2)),
				spec(call_transfer(4, 100), SetOrigin::signed(2)),
			];
			apply_ok(batch_tx(3, BatchMode::StopAtFailure, &specs));
			assert_eq!(balance(4), INITIAL_BALANCE + 300);
			assert_eq!(System::account_nonce(2), 1);
			assert_eq!(
				pallet_events().last(),
				Some(&Event::BatchInterrupted {
					index: 1,
					error: invalid(InvalidTransaction::Stale)
				})
			);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn submitter_can_also_sign_items() {
		new_test_ext().execute_with(|| {
			// The outer `CheckNonce` bumps the submitter's nonce before the items run.
			let specs = [
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(1)), 1),
				spec(call_transfer(4, 100), SetOrigin::signed(2)),
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(1)), 2),
			];
			apply_ok(batch_tx(1, BatchMode::StopAtFailure, &specs));

			assert_eq!(balance(4), INITIAL_BALANCE + 300);
			assert_eq!(balance(1), INITIAL_BALANCE - 200 - fees_paid_by(1));
			assert_eq!(System::account_nonce(1), 3);
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn items_pay_for_themselves_and_the_submitter_for_the_rest() {
		new_test_ext().execute_with(|| {
			let specs = [
				spec(call_foobar(false, 1_000, None), SetOrigin::signed(1)),
				spec(call_foobar(false, 2_000, Some(500)), SetOrigin::signed(2)),
			];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let info = uxt.get_dispatch_info();
			let len = uxt.encoded_size();
			let full_fee = full_fee(&uxt);
			let overhead = overhead(&uxt);
			let (item_0, item_1) = (pipelines_weight(&uxt, &[0]), pipelines_weight(&uxt, &[1]));
			let outer = outer_weight(&uxt);
			assert_eq!(info.call_weight, overhead + item_0 + item_1 + w(3_000));

			let post_info = apply_ok(uxt.clone());

			// The unused call weight is refunded.
			let actual = overhead + item_0 + item_1 + w(1_500) + outer;
			assert_eq!(post_info.actual_weight, Some(actual));
			assert_eq!(fee_paid_by(1), fee(item_0 + w(1_000), items_len(&uxt, &[0])));
			assert_eq!(fee_paid_by(2), fee(item_1 + w(500), items_len(&uxt, &[1])));
			assert_eq!(balance(1), INITIAL_BALANCE - fee_paid_by(1));
			assert_eq!(balance(2), INITIAL_BALANCE - fee_paid_by(2));
			let paid = fee_paid_by(3);
			assert_eq!(paid, fee(overhead + outer, len - items_len(&uxt, &[0, 1])));
			assert!(paid < full_fee);
			assert_eq!(balance(3), INITIAL_BALANCE - paid);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn pipeline_refunds_are_honoured() {
		new_test_ext().execute_with(|| {
			SetOrigin::refund();
			let specs = [spec(call_foobar(false, 1_000, None), SetOrigin::signed(1))];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let pipelines = pipelines_weight(&uxt, &[0]);
			assert!(pipelines.all_gte(SET_ORIGIN_WEIGHT));
			let expected =
				overhead(&uxt) + pipelines - SET_ORIGIN_WEIGHT + w(1_000) + outer_weight(&uxt);
			let submitter = submitter_fee(&uxt);
			let item_len = items_len(&uxt, &[0]);

			let post_info = apply_ok(uxt);

			assert_eq!(post_info.actual_weight, Some(expected));
			assert_eq!(fee_paid_by(1), fee(pipelines - SET_ORIGIN_WEIGHT + w(1_000), item_len));
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
		});
	}

	#[test]
	fn invalid_item_stops_the_batch() {
		new_test_ext().execute_with(|| {
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				// Future nonce.
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(2)), 5),
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
			];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			// The invalid item's pipeline and the bytes of both are paid for by the submitter.
			let expected = overhead(&uxt) +
				pipelines_weight(&uxt, &[0, 1]) +
				specs[0].call.get_dispatch_info().call_weight +
				outer_weight(&uxt);
			let submitter = submitter_fee_with(&uxt, pipelines_weight(&uxt, &[1]), &[0]);

			let post_info = apply_ok(uxt);

			assert_eq!(balance(4), INITIAL_BALANCE + 100);
			assert_eq!(balance(1), INITIAL_BALANCE - 100 - fee_paid_by(1));
			assert_eq!(System::account_nonce(1), 1);
			// The invalid item left no trace.
			assert_eq!(balance(2), INITIAL_BALANCE);
			assert_eq!(System::account_nonce(2), 0);
			assert_eq!(SetOrigin::prepared(), 1);
			assert_eq!(SetOrigin::post_dispatched(), 1);
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(balance(3), INITIAL_BALANCE - submitter);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemCompleted { index: 0 },
					Event::BatchInterrupted {
						index: 1,
						error: invalid(InvalidTransaction::Future)
					},
				]
			);
			assert_eq!(post_info.actual_weight, Some(expected));
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn failed_item_stops_the_batch() {
		new_test_ext().execute_with(|| {
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				spec(call_foobar(true, 1_000, Some(300)), SetOrigin::signed(2)),
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
			];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			// The failed call's actual weight is paid for.
			let expected = overhead(&uxt) +
				pipelines_weight(&uxt, &[0, 1]) +
				specs[0].call.get_dispatch_info().call_weight +
				w(300) + outer_weight(&uxt);
			let item_1 = pipelines_weight(&uxt, &[1]);
			// The item the batch stopped before is on the submitter.
			let submitter = submitter_fee_with(&uxt, Weight::zero(), &[0, 1]);
			let item_1_len = items_len(&uxt, &[1]);

			let post_info = apply_ok(uxt);

			assert_eq!(balance(4), INITIAL_BALANCE + 100);
			assert_eq!(System::account_nonce(1), 1);
			// The failed item's pipeline applied: nonce bumped, fee paid like for a failed
			// transaction.
			assert_eq!(fee_paid_by(2), fee(item_1 + w(300), item_1_len));
			assert_eq!(balance(2), INITIAL_BALANCE - fee_paid_by(2));
			assert_eq!(System::account_nonce(2), 1);
			assert_eq!(SetOrigin::prepared(), 2);
			assert_eq!(SetOrigin::post_dispatched(), 2);
			// The call's effects were reverted.
			assert_eq!(example::Calls::<Test>::get(), 0);
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(balance(3), INITIAL_BALANCE - submitter);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemCompleted { index: 0 },
					Event::BatchInterrupted { index: 1, error: custom() },
				]
			);
			assert_eq!(post_info.actual_weight, Some(expected));
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn force_batch_applies_every_item() {
		new_test_ext().execute_with(|| {
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(2)), 5),
				spec(call_foobar(true, 1_000, None), SetOrigin::signed(2)),
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(1)), 1),
			];
			let uxt = batch_tx(3, BatchMode::All, &specs);
			let expected = overhead(&uxt) +
				pipelines_weight(&uxt, &[0, 1, 2, 3]) +
				specs[0].call.get_dispatch_info().call_weight +
				w(1_000) + specs[3].call.get_dispatch_info().call_weight +
				outer_weight(&uxt);
			// The invalid item's pipeline and bytes are on the submitter.
			let submitter = submitter_fee_with(&uxt, pipelines_weight(&uxt, &[1]), &[0, 2, 3]);

			let post_info = apply_ok(uxt);

			assert_eq!(balance(4), INITIAL_BALANCE + 200);
			assert_eq!(System::account_nonce(1), 2);
			// The failed item is on its signer.
			assert!(fee_paid_by(2) > 0);
			assert_eq!(balance(2), INITIAL_BALANCE - fee_paid_by(2));
			assert_eq!(System::account_nonce(2), 1);
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(balance(3), INITIAL_BALANCE - submitter);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemCompleted { index: 0 },
					Event::ItemFailed { index: 1, error: invalid(InvalidTransaction::Future) },
					Event::ItemFailed { index: 2, error: custom() },
					Event::ItemCompleted { index: 3 },
					Event::BatchCompletedWithErrors,
				]
			);
			assert_eq!(post_info.actual_weight, Some(expected));
			assert_fee_credit_empty();

			// Without failures.
			let specs = [spec(call_transfer(4, 100), SetOrigin::signed(1))];
			apply_ok(batch_tx(3, BatchMode::All, &specs));
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
		});
	}

	#[test]
	fn unauthorized_item_is_invalid() {
		new_test_ext().execute_with(|| {
			let specs = [spec(call_remark(), SetOrigin::none())];
			apply_ok(batch_tx(3, BatchMode::StopAtFailure, &specs));
			assert_eq!(
				pallet_events(),
				vec![Event::BatchInterrupted {
					index: 0,
					error: invalid(InvalidTransaction::UnknownOrigin)
				}]
			);
		});
	}

	#[test]
	fn item_origin_is_enforced() {
		new_test_ext().execute_with(|| {
			let specs = [
				// Signed by 2, claims to be 1.
				expecting(spec(call_transfer(4, 100), SetOrigin::signed(2)), signed_by(1)),
				// Authorized, claims to be signed by 1.
				expecting(spec(call_claim(), SetOrigin::none()), signed_by(1)),
				// Signed by 1, claims to be authorized.
				expecting(spec(call_remark(), SetOrigin::signed(1)), authorized_origin()),
			];
			apply_ok(batch_tx(3, BatchMode::All, &specs));
			let bad_signer = invalid(InvalidTransaction::BadSigner);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemFailed { index: 0, error: bad_signer.clone() },
					Event::ItemFailed { index: 1, error: bad_signer.clone() },
					Event::ItemFailed { index: 2, error: bad_signer },
					Event::BatchCompletedWithErrors,
				]
			);
			assert_eq!(balance(2), INITIAL_BALANCE);
			assert_eq!(System::account_nonce(1), 0);
			assert_eq!(System::account_nonce(2), 0);
			assert_eq!(example::Claims::<Test>::get(), 0);
		});
	}

	#[test]
	fn item_is_bound_to_its_batch() {
		new_test_ext().execute_with(|| {
			// Two identical items of 1 with consecutive nonces, so that swapping them keeps the
			// items hash.
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(1)), 1),
			];
			let mode = BatchMode::StopAtFailure;

			// Alone: the items hash differs.
			let items = build_items(3, 0, mode, &specs);
			apply_ok(signed_tx(3, batch_call(mode, 0, vec![items[0].clone()])));
			// Next to a different item: the items hash differs.
			let items = build_items(3, 1, mode, &specs);
			let other = [specs[0].clone(), spec(call_transfer(5, 100), SetOrigin::signed(2))];
			let other_items = build_items(3, 1, mode, &other);
			apply_ok(signed_tx(
				3,
				batch_call(mode, 1, vec![items[0].clone(), other_items[1].clone()]),
			));
			// Swapped: the index differs.
			let items = build_items(3, 2, mode, &specs);
			apply_ok(signed_tx(3, batch_call(mode, 2, vec![items[1].clone(), items[0].clone()])));
			let bad_proof = invalid(InvalidTransaction::BadProof);
			assert_eq!(
				pallet_events(),
				vec![
					Event::BatchInterrupted { index: 0, error: bad_proof.clone() },
					Event::BatchInterrupted { index: 0, error: bad_proof.clone() },
					Event::BatchInterrupted { index: 0, error: bad_proof },
				]
			);
			assert_eq!(balance(4), INITIAL_BALANCE);
			assert_eq!(System::account_nonce(1), 0);

			// As signed, in order.
			let items = build_items(3, 3, mode, &specs);
			apply_ok(signed_tx(3, batch_call(mode, 3, items)));
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			assert_eq!(balance(4), INITIAL_BALANCE + 200);
		});
	}

	#[test]
	fn item_is_bound_to_mode_submitter_and_nonce() {
		new_test_ext().execute_with(|| {
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				spec(call_transfer(4, 100), SetOrigin::signed(2)),
			];
			// Signed for submitter 3's batch with nonce 1, in `StopAtFailure` mode.
			let items = build_items(3, 1, BatchMode::StopAtFailure, &specs);

			// Submitted by 3 with nonce 0.
			apply_ok(signed_tx(3, batch_call(BatchMode::StopAtFailure, 0, items.clone())));
			assert_eq!(System::account_nonce(3), 1);
			// Submitted by 5 with nonce 1.
			assert_ok!(apply(signed_tx(5, call_transfer(4, 1))).unwrap());
			assert_eq!(System::account_nonce(5), 1);
			apply_ok(signed_tx(5, batch_call(BatchMode::StopAtFailure, 1, items.clone())));
			// Submitted by 3 with nonce 1, in `All` mode.
			apply_ok(signed_tx(3, batch_call(BatchMode::All, 1, items.clone())));
			assert_eq!(
				pallet_events(),
				vec![
					Event::BatchInterrupted {
						index: 0,
						error: invalid(InvalidTransaction::BadProof)
					},
					Event::BatchInterrupted {
						index: 0,
						error: invalid(InvalidTransaction::BadProof)
					},
					Event::ItemFailed { index: 0, error: invalid(InvalidTransaction::BadProof) },
					Event::ItemFailed { index: 1, error: invalid(InvalidTransaction::BadProof) },
					Event::BatchCompletedWithErrors,
				]
			);
			assert_eq!(balance(4), INITIAL_BALANCE + 1);

			// Submitted by 3 with nonce 2 now, which is not what the items were signed for.
			apply_ok(signed_tx(3, batch_call(BatchMode::StopAtFailure, 2, items.clone())));
			assert_eq!(
				pallet_events().last(),
				Some(&Event::BatchInterrupted {
					index: 0,
					error: invalid(InvalidTransaction::BadProof)
				})
			);

			// Signed for exactly this batch.
			let items = build_items(3, 3, BatchMode::StopAtFailure, &specs);
			apply_ok(signed_tx(3, batch_call(BatchMode::StopAtFailure, 3, items)));
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			assert_eq!(balance(4), INITIAL_BALANCE + 201);
		});
	}

	#[test]
	fn items_cannot_nest_batch_calls() {
		new_test_ext().execute_with(|| {
			let inner = batch_call(
				BatchMode::StopAtFailure,
				0,
				build_items(
					3,
					0,
					BatchMode::StopAtFailure,
					&[spec(call_remark(), SetOrigin::signed(1))],
				),
			);
			// Directly: the item is invalid, its pipeline does not even run.
			let specs = [spec(inner.clone(), SetOrigin::signed(1))];
			apply_ok(batch_tx(3, BatchMode::StopAtFailure, &specs));
			assert_eq!(
				pallet_events().last(),
				Some(&Event::BatchInterrupted {
					index: 0,
					error: invalid(InvalidTransaction::Call)
				})
			);
			apply_ok(batch_tx(3, BatchMode::All, &specs));
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompletedWithErrors));
			assert_eq!(SetOrigin::prepared(), 0);
			assert_eq!(System::account_nonce(1), 0);
			// Nested in another call: filtered at dispatch, the item fails.
			let nested = RuntimeCall::Utility(UtilityCall::batch_all { calls: vec![inner] });
			let specs = [spec(nested, SetOrigin::signed(1))];
			apply_ok(batch_tx(3, BatchMode::StopAtFailure, &specs));
			assert_eq!(
				pallet_events().last(),
				Some(&Event::BatchInterrupted {
					index: 0,
					error: failed(frame_system::Error::<Test>::CallFiltered),
				})
			);
			assert_eq!(System::account_nonce(1), 1);
		});
	}

	#[test]
	fn filtered_calls_are_rejected() {
		new_test_ext().execute_with(|| {
			let noop = RuntimeCall::Example(ExampleCall::noop { weight: Weight::from_parts(1, 0) });
			// Directly: the item is invalid, its pipeline does not even run.
			let specs = [spec(noop.clone(), SetOrigin::signed(1))];
			apply_ok(batch_tx(3, BatchMode::StopAtFailure, &specs));
			assert_eq!(
				pallet_events().last(),
				Some(&Event::BatchInterrupted {
					index: 0,
					error: invalid(InvalidTransaction::Call)
				})
			);
			assert_eq!(SetOrigin::prepared(), 0);
			// Nested: the item fails.
			let nested = RuntimeCall::Utility(UtilityCall::batch_all { calls: vec![noop] });
			let specs = [spec(nested, SetOrigin::signed(1))];
			apply_ok(batch_tx(3, BatchMode::StopAtFailure, &specs));
			assert_eq!(
				pallet_events().last(),
				Some(&Event::BatchInterrupted {
					index: 0,
					error: failed(frame_system::Error::<Test>::CallFiltered),
				})
			);
		});
	}

	#[test]
	fn empty_batch_completes() {
		new_test_ext().execute_with(|| {
			for mode in [BatchMode::StopAtFailure, BatchMode::All] {
				let uxt = batch_tx(3, mode, &[]);
				let expected = overhead(&uxt) + outer_weight(&uxt);
				let post_info = apply_ok(uxt);
				assert_eq!(post_info.actual_weight, Some(expected));
				assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			}
		});
	}

	#[test]
	fn batch_requires_a_signed_submitter() {
		new_test_ext().execute_with(|| {
			let call = batch_call(
				BatchMode::StopAtFailure,
				0,
				build_items(
					3,
					0,
					BatchMode::StopAtFailure,
					&[spec(call_remark(), SetOrigin::signed(1))],
				),
			);
			// A general transaction has no signer and nothing in the pipeline authorizes it.
			let uxt = UncheckedExtrinsic::new_transaction(call.clone(), tx_ext(0, 0));
			assert_eq!(apply(uxt), Err(InvalidTransaction::UnknownOrigin.into()));
			// A bare one skips the pipeline; the call fails before any item, charging the
			// overhead only.
			let uxt = UncheckedExtrinsic::new_bare(call);
			let overhead = overhead(&uxt);
			assert_eq!(
				apply(uxt).unwrap(),
				Err(DispatchErrorWithPostInfo {
					post_info: Some(overhead).into(),
					error: BadOrigin.into(),
				})
			);
			assert_eq!(System::account_nonce(1), 0);
		});
	}

	#[test]
	fn call_checks_the_nonce_it_carries() {
		new_test_ext().execute_with(|| {
			let specs = [spec(call_remark(), SetOrigin::signed(1))];
			let items: MultiOriginItemsOf<Test> =
				build_items(3, 0, BatchMode::StopAtFailure, &specs).try_into().unwrap();
			let bad_nonce = DispatchErrorWithPostInfo {
				post_info: Some(Pallet::<Test>::overhead_weight(1, items.encoded_size() as u32))
					.into(),
				error: Error::<Test>::BadNonce.into(),
			};
			// Signed, but 3 has not sent the transaction with nonce 0 yet.
			assert_noop!(
				UtilityExt::batch_multi_origin(RuntimeOrigin::signed(3), items.clone(), 0),
				bad_nonce
			);
			// Once `CheckNonce` bumped it, the batch is that transaction's call.
			System::inc_account_nonce(3);
			assert_ok!(UtilityExt::batch_multi_origin(RuntimeOrigin::signed(3), items.clone(), 0));
			assert_eq!(System::account_nonce(1), 1);
			// Even an unbumpable nonce is just a wrong one.
			assert_noop!(
				UtilityExt::batch_multi_origin(RuntimeOrigin::signed(3), items, u32::MAX),
				bad_nonce
			);

			// A transaction carrying another nonce than its own fails before any item, charging
			// the overhead only.
			let items = build_items(3, 0, BatchMode::StopAtFailure, &specs);
			let uxt = signed_tx(3, batch_call(BatchMode::StopAtFailure, 0, items));
			let len = uxt.encoded_size();
			let charged = overhead(&uxt) + outer_weight(&uxt);
			let err = apply(uxt).unwrap().unwrap_err();
			assert_eq!(err.error, Error::<Test>::BadNonce.into());
			assert_eq!(err.post_info.actual_weight, Some(charged));
			assert_eq!(fee_paid_by(3), fee(charged, len));
			assert_eq!(System::account_nonce(3), 2);
			assert_eq!(System::account_nonce(1), 1);
		});
	}

	#[test]
	fn batch_is_charged_like_any_transaction() {
		new_test_ext().execute_with(|| {
			// The item is rejected after its call ran: nothing is refunded.
			SetOrigin::fail_post_dispatch();
			let specs = [spec(call_foobar(false, 1_000, None), SetOrigin::signed(1))];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let info = uxt.get_dispatch_info();
			let len = uxt.encoded_size();
			let full_fee = full_fee(&uxt);
			assert_eq!(full_fee, TransactionPayment::compute_fee(len as u32, &info, 0));

			let validity = validate(uxt.clone()).unwrap();
			assert_eq!(
				validity.priority,
				ChargeTransactionPayment::<Test>::get_priority(&info, len, 0, full_fee)
			);

			// Every declared weight is used: the full fee is paid.
			let post_info = apply_ok(uxt);
			let paid = fee_paid_by(3);
			assert_eq!(
				paid,
				TransactionPayment::compute_actual_fee(len as u32, &info, &post_info, 0)
			);
			assert_eq!(paid, full_fee);
			assert_eq!(balance(3), INITIAL_BALANCE - paid);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn item_whose_post_dispatch_fails_stops_the_batch() {
		new_test_ext().execute_with(|| {
			SetOrigin::fail_post_dispatch();
			let specs = [
				spec(call_transfer(4, 100), SetOrigin::signed(1)),
				spec(call_transfer(4, 100), SetOrigin::signed(2)),
			];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			// The call ran before the rollback.
			let expected = overhead(&uxt) +
				pipelines_weight(&uxt, &[0]) +
				specs[0].call.get_dispatch_info().call_weight +
				outer_weight(&uxt);
			let submitter = fee(expected, uxt.encoded_size());

			let post_info = apply_ok(uxt);

			// The item's settlement was rolled back with it.
			assert_eq!(balance(4), INITIAL_BALANCE);
			assert_eq!(balance(1), INITIAL_BALANCE);
			assert_eq!(System::account_nonce(1), 0);
			assert_eq!(SetOrigin::post_dispatched(), 0);
			assert_eq!(
				pallet_events(),
				vec![Event::BatchInterrupted {
					index: 0,
					error: invalid(SET_ORIGIN_POST_DISPATCH_ERROR)
				}]
			);
			assert_eq!(post_info.actual_weight, Some(expected));
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(balance(3), INITIAL_BALANCE - submitter);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn items_whose_post_dispatch_fails_are_rolled_back_and_paid_for() {
		new_test_ext().execute_with(|| {
			SetOrigin::fail_post_dispatch();
			let specs = [
				spec(call_foobar(false, 1_000, Some(300)), SetOrigin::signed(1)),
				spec(call_transfer(4, 100), SetOrigin::signed(2)),
				spec(call_free(false), SetOrigin::signed(3)),
			];
			let uxt = batch_tx(5, BatchMode::All, &specs);
			let len = uxt.encoded_size();
			// Every call is charged to the submitter at its declared weight, the free one
			// included.
			let expected = overhead(&uxt) +
				pipelines_weight(&uxt, &[0, 1, 2]) +
				w(1_000) + specs[1].call.get_dispatch_info().call_weight +
				w(10) + outer_weight(&uxt);

			let post_info = apply_ok(uxt);

			assert_eq!(post_info.actual_weight, Some(expected));
			assert_eq!(fee_paid_by(5), fee(expected, len));
			let error = invalid(SET_ORIGIN_POST_DISPATCH_ERROR);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemFailed { index: 0, error: error.clone() },
					Event::ItemFailed { index: 1, error: error.clone() },
					Event::ItemFailed { index: 2, error },
					Event::BatchCompletedWithErrors,
				]
			);
			// The calls ran and were rolled back with their items, events and settlements
			// included.
			assert_eq!(example::Calls::<Test>::get(), 0);
			assert_eq!(balance(4), INITIAL_BALANCE);
			assert_eq!(balance(1), INITIAL_BALANCE);
			assert_eq!(balance(2), INITIAL_BALANCE);
			assert_eq!(balance(3), INITIAL_BALANCE);
			assert!(!System::events().iter().any(|record| matches!(
				record.event,
				RuntimeEvent::Balances(pallet_balances::Event::Transfer { .. })
			)));
			assert_eq!(System::account_nonce(1), 0);
			assert_eq!(System::account_nonce(2), 0);
			assert_eq!(System::account_nonce(3), 0);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn exhausted_storage_transactions_make_the_item_invalid() {
		fn nested(depth: u32, f: Box<dyn FnOnce()>) {
			if depth == 0 {
				return f();
			}
			with_transaction::<(), DispatchError, _>(|| {
				nested(depth - 1, f);
				TransactionOutcome::Commit(Ok(()))
			})
			.expect("below the limit");
		}
		new_test_ext().execute_with(|| {
			let specs = [spec(call_transfer(4, 100), SetOrigin::signed(1))];
			let items: MultiOriginItemsOf<Test> =
				build_items(3, 0, BatchMode::StopAtFailure, &specs).try_into().unwrap();
			let expected = Pallet::<Test>::overhead_weight(1, items.encoded_size() as u32) +
				items[0].extension.weight(&items[0].call);
			System::inc_account_nonce(3);
			// The call itself takes the last layer: the item's is one too many.
			nested(
				frame_support::storage::transactional::TRANSACTIONAL_LIMIT as u32 - 1,
				Box::new(move || {
					let post_info =
						UtilityExt::batch_multi_origin(RuntimeOrigin::signed(3), items, 0).unwrap();
					assert_eq!(post_info.actual_weight, Some(expected));
				}),
			);
			assert_eq!(
				pallet_events(),
				vec![Event::BatchInterrupted {
					index: 0,
					error: invalid(InvalidTransaction::ExhaustsResources)
				}]
			);
			assert_eq!(balance(4), INITIAL_BALANCE);
			assert_eq!(System::account_nonce(1), 0);
		});
	}

	#[test]
	fn item_implication_encoding_is_stable() {
		// The encoding is part of the signing contract and must not change: prefix, mode,
		// submitter, nonce, index, hash of the SCALE encoded list of `(call, origin)` pairs.
		let call = call_remark();
		let origin = signed_by(1);
		let items_hash = Pallet::<Test>::items_hash([(&call, &origin), (&call, &origin)]);
		assert_eq!(
			items_hash,
			sp_io::hashing::blake2_256(&vec![(&call, &origin), (&call, &origin)].encode()).into()
		);
		let implication = ItemImplication {
			mode: BatchMode::All,
			submitter: &0x0102030405060708u64,
			nonce: 9u32,
			index: 2,
			items_hash: &items_hash,
		};
		let mut expected = vec![
			0x5f, 0x6d, 0x62, 0x61, 0x74, 0x63, 0x68, 0x5f, // "_mbatch_"
			0x01, // `BatchMode::All`
			0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // submitter
			0x09, 0x00, 0x00, 0x00, // nonce
			0x02, 0x00, 0x00, 0x00, // index
		];
		expected.extend(items_hash.as_bytes());
		assert_eq!(implication.encode(), expected);
	}

	#[test]
	fn free_items_cost_nobody() {
		new_test_ext().execute_with(|| {
			// Two free items, one of which fails, and a paying one.
			let specs = [
				spec(call_free(false), SetOrigin::signed(1)),
				spec(call_free(true), SetOrigin::signed(2)),
				with_nonce(spec(call_foobar(false, 1_000, None), SetOrigin::signed(1)), 1),
			];
			let uxt = batch_tx(3, BatchMode::All, &specs);
			let info = uxt.get_dispatch_info();
			let full_fee = full_fee(&uxt);
			assert_eq!(info.pays_fee, Pays::Yes);
			// The block accounts for the free items, nobody pays for them.
			let submitter = submitter_fee(&uxt);
			let item_2 = pipelines_weight(&uxt, &[2]);
			let item_2_len = items_len(&uxt, &[2]);
			let expected =
				overhead(&uxt) + pipelines_weight(&uxt, &[0, 1, 2]) + w(1_020) + outer_weight(&uxt);

			let post_info = apply_ok(uxt);

			assert_eq!(post_info.actual_weight, Some(expected));
			let paid = fee_paid_by(3);
			assert_eq!(paid, submitter);
			assert!(paid < full_fee);
			assert_eq!(fee_paid_by(2), 0);
			assert_eq!(balance(1), INITIAL_BALANCE - fee(item_2 + w(1_000), item_2_len));
			assert_eq!(balance(2), INITIAL_BALANCE);
			assert_eq!(System::account_nonce(1), 2);
			assert_eq!(System::account_nonce(2), 1);
			assert_eq!(balance(3), INITIAL_BALANCE - paid);
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompletedWithErrors));
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn items_are_refunded_when_the_batch_stops() {
		new_test_ext().execute_with(|| {
			// A free item, then an invalid one which stops the batch.
			let specs = [
				spec(call_free(false), SetOrigin::signed(1)),
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(2)), 5),
				spec(call_foobar(false, 1_000, None), SetOrigin::signed(1)),
			];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let expected =
				overhead(&uxt) + pipelines_weight(&uxt, &[0, 1]) + w(10) + outer_weight(&uxt);
			// The free item is refunded, the invalid item's pipeline and the bytes of the other
			// two are not.
			let submitter = submitter_fee_with(&uxt, pipelines_weight(&uxt, &[1]), &[0]);

			let post_info = apply_ok(uxt);

			assert_eq!(post_info.actual_weight, Some(expected));
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(
				pallet_events().last(),
				Some(&Event::BatchInterrupted {
					index: 1,
					error: invalid(InvalidTransaction::Future)
				})
			);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn stale_refundable_share_is_ignored() {
		new_test_ext().execute_with(|| {
			// Left behind by a batch dispatched outside a batch transaction.
			RefundableShare::<Test>::put(Share { weight: Weight::MAX, len: u32::MAX });
			// Other transactions do not look at it.
			let uxt = signed_tx(1, call_transfer(4, 100));
			let transfer_fee = full_fee(&uxt);
			assert_ok!(apply(uxt).unwrap());
			assert_eq!(fee_paid_by(1), transfer_fee);
			assert!(RefundableShare::<Test>::get().is_some());
			assert_eq!(TransactionPayment::remaining_txfee::<u64>(), 0);
		});
	}

	#[test]
	fn without_the_batch_extension_the_submitter_pays_in_full() {
		new_test_ext().execute_with(|| {
			let specs = [spec(call_foobar(false, 1_000, None), SetOrigin::signed(1))];
			let call = batch_call(
				BatchMode::StopAtFailure,
				0,
				build_items(3, 0, BatchMode::StopAtFailure, &specs),
			);
			let (post_info, len, paid) = apply_plain(3, call);
			// The item paid for itself and the submitter for it again.
			assert!(fee_paid_by(1) > 0);
			assert_eq!(paid, fee(post_info.actual_weight.unwrap(), len));
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			assert!(RefundableShare::<Test>::get().is_some());
		});
	}

	#[test]
	fn batch_transaction_clears_the_stale_share_in_prepare() {
		new_test_ext().execute_with(|| {
			RefundableShare::<Test>::put(Share { weight: Weight::MAX, len: u32::MAX });
			let specs = [spec(call_free(false), SetOrigin::signed(1))];
			let call = batch_call(
				BatchMode::StopAtFailure,
				0,
				build_items(3, 0, BatchMode::StopAtFailure, &specs),
			);
			let ext: MultiOriginBatch<Test, ChargeTransactionPayment<Test>> =
				ChargeTransactionPayment::from(0).into();
			let info = call.get_dispatch_info();
			let len = call.encoded_size();

			let (_, val, origin) = ext
				.validate_only(
					RuntimeOrigin::signed(3),
					&call,
					&info,
					len,
					TransactionSource::External,
					0,
				)
				.unwrap();
			assert!(RefundableShare::<Test>::get().is_some());
			ext.prepare(val, &origin, &call, &info, len).unwrap();
			assert!(RefundableShare::<Test>::get().is_none());
		});
	}

	#[test]
	fn nested_batch_is_paid_in_full_and_leaves_its_share_behind() {
		new_test_ext().execute_with(|| {
			let inner = batch_call(
				BatchMode::StopAtFailure,
				0,
				build_items(
					3,
					0,
					BatchMode::StopAtFailure,
					&[spec(call_foobar(false, 1_000, None), SetOrigin::signed(1))],
				),
			);
			let uxt =
				signed_tx(3, RuntimeCall::Utility(UtilityCall::batch_all { calls: vec![inner] }));
			let len = uxt.encoded_size();

			let post_info = apply(uxt).unwrap().unwrap();

			// The outer `CheckNonce` bumped 3's nonce. The transaction's call is not a batch:
			// nothing is refunded.
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			assert_eq!(System::account_nonce(1), 1);
			assert!(fee_paid_by(1) > 0);
			assert_eq!(balance(1), INITIAL_BALANCE - fee_paid_by(1));
			assert_eq!(fee_paid_by(3), fee(post_info.actual_weight.unwrap(), len));
			assert_eq!(balance(3), INITIAL_BALANCE - fee_paid_by(3));
			assert!(RefundableShare::<Test>::get().is_some());
			assert_eq!(TransactionPayment::remaining_txfee::<u64>(), 0);
		});
	}

	#[test]
	fn each_batch_refunds_its_own_items() {
		new_test_ext().execute_with(|| {
			let specs = [spec(call_foobar(false, 1_000, None), SetOrigin::signed(1))];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let submitter = submitter_fee(&uxt);
			apply_ok(uxt);
			assert_eq!(fee_paid_by(3), submitter);

			// An invalid item: nothing of this batch is refunded.
			let specs = [with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(2)), 5)];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let submitter = submitter_fee_with(&uxt, pipelines_weight(&uxt, &[0]), &[]);
			apply_ok(uxt);
			assert_eq!(fee_paid_by(3), submitter);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn batch_extension_passes_other_transactions_through() {
		new_test_ext().execute_with(|| {
			// The same transactions through the wrapped and the plain pipeline, by accounts with
			// the same balance.
			for (call, wrapped, plain) in [(call_free(false), 1, 2), (call_transfer(4, 100), 3, 5)]
			{
				let post_info = apply(signed_tx(wrapped, call.clone())).unwrap().unwrap();
				let paid = fee_paid_by(wrapped);
				let (plain_post_info, _, plain_paid) = apply_plain(plain, call);
				assert_eq!(post_info, plain_post_info);
				assert_eq!(paid, plain_paid);
				assert_eq!(balance(wrapped), balance(plain));
			}
			assert_eq!(fee_paid_by(1), 0);
			assert!(fee_paid_by(3) > 0);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn batch_extension_is_invisible() {
		type Inner = ChargeTransactionPayment<Test>;
		type Wrapped = MultiOriginBatch<Test, Inner>;
		assert_eq!(
			<Wrapped as TransactionExtension<RuntimeCall>>::IDENTIFIER,
			<Inner as TransactionExtension<RuntimeCall>>::IDENTIFIER
		);
		assert_eq!(scale_info::meta_type::<Wrapped>(), scale_info::meta_type::<Inner>());
		let (wrapped, inner) = (Wrapped::metadata(), Inner::metadata());
		assert_eq!(
			wrapped.iter().map(|m| (m.identifier, m.ty, m.implicit)).collect::<Vec<_>>(),
			inner.iter().map(|m| (m.identifier, m.ty, m.implicit)).collect::<Vec<_>>()
		);
		assert_eq!(Wrapped::from(Inner::from(7)).encode(), Inner::from(7).encode());
		assert_eq!(Wrapped::from(Inner::from(7)).implicit(), Inner::from(7).implicit());
	}

	#[test]
	fn authorization_is_checked_against_earlier_items() {
		new_test_ext().execute_with(|| {
			let specs = [authorized(call_claim()), authorized(call_claim())];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			// The applied item is refunded though its origin paid nothing.
			let submitter = submitter_fee_with(&uxt, pipelines_weight(&uxt, &[1]), &[0]);
			apply_ok(uxt);

			// The first claim applied, the second was authorized against the state it left.
			assert_eq!(example::Claims::<Test>::get(), 1);
			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemCompleted { index: 0 },
					Event::BatchInterrupted { index: 1, error: invalid(InvalidTransaction::Stale) },
				]
			);
		});
	}

	#[test]
	fn item_whose_signer_cannot_pay_is_invalid() {
		new_test_ext().execute_with(|| {
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 1));
			let specs = [
				spec(call_foobar(false, 1_000, None), SetOrigin::signed(1)),
				spec(call_foobar(false, 1_000, None), SetOrigin::signed(2)),
			];
			let uxt = batch_tx(3, BatchMode::All, &specs);
			let submitter = submitter_fee_with(&uxt, pipelines_weight(&uxt, &[0]), &[1]);
			apply_ok(uxt);
			assert_eq!(
				pallet_events(),
				vec![
					Event::ItemFailed { index: 0, error: invalid(InvalidTransaction::Payment) },
					Event::ItemCompleted { index: 1 },
					Event::BatchCompletedWithErrors,
				]
			);
			assert_eq!(balance(1), 1);
			assert_eq!(System::account_nonce(1), 0);
			assert_eq!(fee_paid_by(3), submitter);
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn items_see_their_own_fee_not_the_submitters() {
		new_test_ext().execute_with(|| {
			// A transaction's call sees the transaction's withdrawn fee.
			let uxt = signed_tx(1, call_observe());
			let transaction_fee = full_fee(&uxt);
			assert_ok!(apply(uxt).unwrap());
			assert_eq!(example::ObservedFeeCredit::<Test>::get(), transaction_fee);

			// An item sees its own, whether the batch is the transaction's call or nested.
			let specs = [spec(call_observe(), SetOrigin::signed(1))];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			let item_fee = fee(pipelines_weight(&uxt, &[0]), items_len(&uxt, &[0]));
			assert!(item_fee < full_fee(&uxt));
			apply_ok(uxt);
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			assert_eq!(example::ObservedFeeCredit::<Test>::get(), item_fee);

			let specs = [spec(call_observe(), SetOrigin::signed(1))];
			let (mode, nonce) = (BatchMode::StopAtFailure, System::account_nonce(3));
			let items = build_items(3, nonce, mode, &specs);
			let item_fee = fee(items[0].extension.weight(&items[0].call), items[0].encoded_size());
			let inner = batch_call(mode, nonce, items);
			let uxt =
				signed_tx(3, RuntimeCall::Utility(UtilityCall::batch_all { calls: vec![inner] }));
			assert_ok!(apply(uxt).unwrap());
			assert_eq!(pallet_events().last(), Some(&Event::BatchCompleted));
			assert_eq!(example::ObservedFeeCredit::<Test>::get(), item_fee);
			assert_eq!(TransactionPayment::remaining_txfee::<u64>(), 0);
		});
	}

	#[test]
	fn tip_stays_with_the_submitter() {
		new_test_ext().execute_with(|| {
			let specs = [spec(call_foobar(false, 1_000, None), SetOrigin::signed(1))];
			let (mode, nonce) = (BatchMode::StopAtFailure, System::account_nonce(3));
			let call = batch_call(mode, nonce, build_items(3, nonce, mode, &specs));
			let uxt = tipped_tx(3, call, 50);
			let submitter = submitter_fee(&uxt) + 50;
			let item = pipelines_weight(&uxt, &[0]) + w(1_000);
			let item_len = items_len(&uxt, &[0]);

			apply_ok(uxt);

			assert_eq!(fee_paid_by(3), submitter);
			assert_eq!(balance(3), INITIAL_BALANCE - submitter);
			assert_eq!(fee_paid_by(1), fee(item, item_len));
			assert_fee_credit_empty();
		});
	}

	#[test]
	fn items_are_not_validated_in_the_pool() {
		new_test_ext().execute_with(|| {
			// Invalid items, failing items and unauthorized items all validate.
			let specs = [
				with_nonce(spec(call_transfer(4, 100), SetOrigin::signed(1)), 9),
				spec(call_foobar(true, 1_000, None), SetOrigin::signed(2)),
				spec(call_remark(), SetOrigin::none()),
			];
			let uxt = batch_tx(3, BatchMode::StopAtFailure, &specs);
			assert_ok!(validate(uxt.clone()));
			assert_eq!(SetOrigin::prepared(), 0);
			assert_eq!(System::account_nonce(1), 0);

			// A submitter who cannot pay is rejected.
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 3, 1));
			assert_eq!(validate(uxt), Err(InvalidTransaction::Payment.into()));
		});
	}
}
