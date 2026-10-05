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
	traits::{ConstU32, Everything},
	weights::Weight,
};
use frame_system::EnsureRoot;
use pallet_collective::{EnsureProportionAtLeast, Instance1};
use sp_runtime::BuildStorage;

type BlockNumber = u64;

// example module to test behaviors.
#[frame_support::pallet(dev_mode)]
pub mod example {
	use frame_support::{dispatch::WithPostDispatchInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type CouncilOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(*_weight)]
		pub fn noop(_origin: OriginFor<T>, _weight: Weight) -> DispatchResult {
			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight((Weight::from_parts(10, 0), Pays::No))]
		pub fn free(origin: OriginFor<T>, err: bool) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!err, DispatchError::Other("custom"));
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
			if err {
				let error: DispatchError = "custom".into();
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
	}

	/// How many times `council_only` was dispatched.
	#[pallet::storage]
	pub type CouncilCalls<T: Config> = StorageValue<_, u32, ValueQuery>;
}

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
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

parameter_types! {
	pub const MotionDuration: BlockNumber = 3;
	pub const MaxProposals: u32 = 100;
	pub const MaxMembers: u32 = 100;
	pub MaxProposalWeight: Weight = sp_runtime::Perbill::from_percent(50) * BlockWeights::get().max_block;
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
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type InnerExtension = InnerExtension;
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

/// The inner extension pipeline of `batch_multi_origin` items in tests.
pub type InnerExtension = (MultiOriginBatchMarker<Test>, SetOrigin, frame_system::CheckNonce<Test>);

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkItems;
#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper<Test> for BenchmarkItems {
	fn item(call: RuntimeCall) -> MultiOriginItemOf<Test> {
		MultiOriginItem::new(
			call,
			(
				MultiOriginBatchMarker::new(),
				SetOrigin::signed(1),
				frame_system::CheckNonce::from(0),
			),
			OriginCaller::system(frame_system::RawOrigin::Signed(1)),
		)
	}
}

use multi_origin_mock::*;

/// Mock pieces for the `batch_multi_origin` tests.
mod multi_origin_mock {
	use super::*;
	use frame_support::{
		pallet_prelude::{Pays, TransactionSource},
		storage,
	};
	use sp_runtime::{
		traits::{Implication, TransactionExtension, ValidateResult},
		transaction_validity::{TransactionValidityError, ValidTransaction},
		DispatchResult,
	};

	/// The runtime's transaction extension pipeline in tests.
	pub type TxExtension = (MultiOriginBatch<Test>, frame_system::CheckWeight<Test>);
	pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
		u64,
		RuntimeCall,
		sp_runtime::testing::TestSignature,
		TxExtension,
	>;

	/// Keys of the counters `SetOrigin` keeps in storage.
	pub const SET_ORIGIN_PREPARED: &[u8] = b"SetOrigin::prepared";
	pub const SET_ORIGIN_POST_DISPATCHED: &[u8] = b"SetOrigin::post_dispatched";
	/// The sum of the actual weights `SetOrigin::post_dispatch` was given.
	pub const SET_ORIGIN_SEEN_WEIGHT: &[u8] = b"SetOrigin::seen_weight";
	/// How many times `SetOrigin::post_dispatch` was given a `Pays::No` item.
	pub const SET_ORIGIN_SEEN_PAYS_NO: &[u8] = b"SetOrigin::seen_pays_no";
	/// The weight `SetOrigin` declares and the part of it it does not spend.
	pub const SET_ORIGIN_WEIGHT: Weight = Weight::from_parts(7, 0);
	pub const SET_ORIGIN_UNSPENT: Weight = Weight::from_parts(2, 0);

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
		pub fn seen_weight() -> Weight {
			storage::unhashed::get_or_default(SET_ORIGIN_SEEN_WEIGHT)
		}
		pub fn seen_pays_no() -> u32 {
			storage::unhashed::get_or_default(SET_ORIGIN_SEEN_PAYS_NO)
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
			// Signed origins get their account as priority.
			let priority = match &self.0 {
				OriginCaller::system(frame_system::RawOrigin::Signed(who)) => *who,
				_ => 0,
			};
			origin.set_caller(self.0.clone());
			Ok((ValidTransaction { priority, ..Default::default() }, (), origin))
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
			info: &DispatchInfo,
			post_info: &PostDispatchInfo,
			_: usize,
			_: &DispatchResult,
		) -> Result<Weight, TransactionValidityError> {
			Self::bump(SET_ORIGIN_POST_DISPATCHED);
			if post_info.pays_fee(info) == Pays::No {
				Self::bump(SET_ORIGIN_SEEN_PAYS_NO);
			}
			storage::unhashed::put(
				SET_ORIGIN_SEEN_WEIGHT,
				&Self::seen_weight().saturating_add(post_info.actual_weight.unwrap_or_default()),
			);
			Ok(SET_ORIGIN_UNSPENT)
		}
	}
}

type ExampleCall = example::Call<Test>;
use pallet_balances::Call as BalancesCall;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10), (2, 10), (3, 10), (4, 10), (5, 2)],
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

fn call_transfer(dest: u64, value: u64) -> RuntimeCall {
	RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value })
}

mod multi_origin {
	use super::*;
	use crate::extension::Val;
	use frame_support::{
		assert_err, assert_noop, assert_ok, dispatch::GetDispatchInfo,
		pallet_prelude::TransactionSource,
	};
	use frame_system::Call as SystemCall;
	use sp_runtime::{
		traits::{
			Applyable, BadOrigin, Checkable, Dispatchable, IdentityLookup, TransactionExtension,
			TxBaseImplication,
		},
		transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
		ApplyExtrinsicResultWithInfo, DispatchError,
	};

	type UtilityCall = pallet_utility::Call<Test>;
	type UtilityExtCall = crate::Call<Test>;

	fn item(call: RuntimeCall, set_origin: SetOrigin) -> MultiOriginItemOf<Test> {
		let nonce = match &set_origin.0 {
			OriginCaller::system(frame_system::RawOrigin::Signed(who)) => {
				System::account_nonce(who)
			},
			_ => 0,
		};
		item_with_nonce(call, set_origin, nonce)
	}

	fn item_with_nonce(
		call: RuntimeCall,
		set_origin: SetOrigin,
		nonce: u32,
	) -> MultiOriginItemOf<Test> {
		let origin = set_origin.0.clone();
		MultiOriginItem::new(
			call,
			(MultiOriginBatchMarker::new(), set_origin, frame_system::CheckNonce::from(nonce)),
			origin,
		)
	}

	fn expecting(mut item: MultiOriginItemOf<Test>, who: u64) -> MultiOriginItemOf<Test> {
		item.origin = OriginCaller::system(frame_system::RawOrigin::Signed(who));
		item
	}

	fn batch_call(items: Vec<MultiOriginItemOf<Test>>) -> RuntimeCall {
		RuntimeCall::UtilityExt(UtilityExtCall::batch_multi_origin {
			items: items.try_into().expect("at most `MaxMultiOriginBatch` items"),
		})
	}

	fn general_tx(call: RuntimeCall) -> UncheckedExtrinsic {
		let ext: TxExtension = (MultiOriginBatch::new(), frame_system::CheckWeight::new());
		UncheckedExtrinsic::new_transaction(call, ext)
	}

	fn check(
		uxt: UncheckedExtrinsic,
	) -> (<UncheckedExtrinsic as Checkable<IdentityLookup<u64>>>::Checked, DispatchInfo, usize) {
		let info = uxt.get_dispatch_info();
		let len = uxt.encoded_size();
		let xt =
			<UncheckedExtrinsic as Checkable<IdentityLookup<u64>>>::check(uxt, &Default::default())
				.expect("general transactions always check");
		(xt, info, len)
	}

	fn validate(call: RuntimeCall) -> Result<ValidTransaction, TransactionValidityError> {
		let (xt, info, len) = check(general_tx(call));
		xt.validate::<Test>(TransactionSource::External, &info, len)
	}

	fn apply(call: RuntimeCall) -> ApplyExtrinsicResultWithInfo<PostDispatchInfo> {
		let (xt, info, len) = check(general_tx(call));
		xt.apply::<Test>(&info, len)
	}

	fn assert_transient_storage_empty() {
		assert!(MultiOrigins::<Test>::get().is_none());
		assert!(MultiOriginPostInfos::<Test>::get().is_none());
	}

	#[test]
	fn dispatches_each_item_with_its_own_origin() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![
				item(call_transfer(5, 3), SetOrigin::signed(1)),
				item(call_transfer(5, 4), SetOrigin::signed(2)),
				item(RuntimeCall::Example(ExampleCall::council_only {}), SetOrigin::council()),
			]);

			let validity = validate(call.clone()).unwrap();
			// One nonce tag per signed item.
			assert_eq!(validity.provides.len(), 2);

			let post_info = apply(call).unwrap().unwrap();
			assert_eq!(Balances::free_balance(1), 7);
			assert_eq!(Balances::free_balance(2), 6);
			assert_eq!(Balances::free_balance(5), 9);
			assert_eq!(example::CouncilCalls::<Test>::get(), 1);
			System::assert_has_event(Event::BatchCompleted.into());
			assert_eq!(
				System::events()
					.iter()
					.filter(|e| e.event == Event::ItemCompleted.into())
					.count(),
				3
			);
			// Every signed item consumed a nonce.
			assert_eq!(System::account_nonce(1), 1);
			assert_eq!(System::account_nonce(2), 1);
			// Every item's inner pipeline ran fully, exactly once.
			assert_eq!(SetOrigin::prepared(), 3);
			assert_eq!(SetOrigin::post_dispatched(), 3);
			assert_transient_storage_empty();
			assert!(post_info.actual_weight.is_some());
		});
	}

	#[test]
	fn unauthorized_item_is_unknown_origin() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![
				item(call_transfer(5, 3), SetOrigin::signed(1)),
				item(call_transfer(5, 3), SetOrigin::none()),
			]);
			assert_eq!(validate(call.clone()), Err(InvalidTransaction::UnknownOrigin.into()));
			assert_eq!(apply(call).unwrap_err(), InvalidTransaction::UnknownOrigin.into());
			assert_eq!(Balances::free_balance(1), 10);
			assert_eq!(System::account_nonce(1), 0);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn failing_item_rolls_back_the_batch_but_not_the_items_bookkeeping() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![
				item(call_transfer(5, 3), SetOrigin::signed(1)),
				item(
					RuntimeCall::Example(ExampleCall::foobar {
						err: true,
						start_weight: Weight::from_parts(10, 0),
						end_weight: None,
					}),
					SetOrigin::signed(2),
				),
			]);
			// Valid: the items are authorized, dispatch failures are not validation failures.
			assert_ok!(validate(call.clone()));

			let (xt, info, len) = check(general_tx(call.clone()));
			let items = Pallet::<Test>::multi_origin_items(&call).unwrap();
			let worst_case = Pallet::<Test>::multi_origin_item_infos(items, &info, len)
				.iter()
				.fold(Weight::zero(), |w, (i, _)| w.saturating_add(i.total_weight()));
			let result = xt.apply::<Test>(&info, len).unwrap();
			assert_eq!(result.unwrap_err().error, DispatchError::Other("custom"));
			// The transfer of the first item was rolled back...
			assert_eq!(Balances::free_balance(1), 10);
			assert_eq!(Balances::free_balance(5), 2);
			assert!(!System::events().iter().any(|e| e.event == Event::BatchCompleted.into()));
			// ... but the items' pipelines ran fully, as for any failed transaction.
			assert_eq!(System::account_nonce(1), 1);
			assert_eq!(System::account_nonce(2), 1);
			assert_eq!(SetOrigin::prepared(), 2);
			assert_eq!(SetOrigin::post_dispatched(), 2);
			// Every item is charged its worst case, including the one which failed.
			assert_eq!(SetOrigin::seen_weight(), worst_case);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn items_cannot_nest_batch_multi_origin() {
		new_test_ext().execute_with(|| {
			let nested = batch_call(vec![]);
			let filtered: DispatchError = frame_system::Error::<Test>::CallFiltered.into();

			// Directly: the item fails and so does the batch.
			let result =
				apply(batch_call(vec![item(nested.clone(), SetOrigin::signed(1))])).unwrap();
			assert_eq!(result.unwrap_err().error, filtered);
			assert_transient_storage_empty();

			// Through another call: the filter follows the origin.
			let via_batch = RuntimeCall::Utility(UtilityCall::batch { calls: vec![nested] });
			assert_ok!(apply(batch_call(vec![item(via_batch, SetOrigin::signed(1))])).unwrap());
			System::assert_has_event(
				pallet_utility::Event::BatchInterrupted { index: 0, error: filtered }.into(),
			);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn filtered_calls_are_rejected() {
		new_test_ext().execute_with(|| {
			let noop = RuntimeCall::Example(ExampleCall::noop { weight: Weight::zero() });
			let filtered: DispatchError = frame_system::Error::<Test>::CallFiltered.into();

			// Directly: rejected before it ever reaches a block.
			let call = batch_call(vec![
				item(call_transfer(5, 3), SetOrigin::signed(1)),
				item(noop.clone(), SetOrigin::signed(2)),
			]);
			assert_eq!(validate(call.clone()), Err(InvalidTransaction::Call.into()));
			assert_eq!(apply(call).unwrap_err(), InvalidTransaction::Call.into());
			assert_eq!(Balances::free_balance(1), 10);
			assert_eq!(System::account_nonce(1), 0);

			// Nested in another call: the filter follows the item's origin to dispatch.
			let via_batch_all =
				RuntimeCall::Utility(UtilityCall::batch_all { calls: vec![noop.clone()] });
			let call = batch_call(vec![item(via_batch_all, SetOrigin::signed(1))]);
			assert_ok!(validate(call.clone()));
			assert_eq!(apply(call).unwrap().unwrap_err().error, filtered);

			// The filter only applies within multi-origin batches.
			assert_ok!(noop.dispatch(RuntimeOrigin::signed(1)));
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn call_requires_the_multi_origin_batch_origin() {
		new_test_ext().execute_with(|| {
			let items: MultiOriginItemsOf<Test> =
				vec![item(call_transfer(5, 3), SetOrigin::signed(1))].try_into().unwrap();
			assert_noop!(
				UtilityExt::batch_multi_origin(RuntimeOrigin::signed(1), items.clone()),
				BadOrigin
			);
			assert_noop!(
				UtilityExt::batch_multi_origin(RuntimeOrigin::root(), items.clone()),
				BadOrigin
			);
			assert_noop!(
				UtilityExt::batch_multi_origin(RuntimeOrigin::none(), items.clone()),
				BadOrigin
			);

			// The right origin, but no origins handed over by the extension.
			let origin: RuntimeOrigin = Origin::MultiOriginBatch.into();
			assert_noop!(
				UtilityExt::batch_multi_origin(origin.clone(), items.clone()),
				Error::<Test>::MissingOrigins
			);
			MultiOrigins::<Test>::put(vec![
				OriginCaller::system(frame_system::RawOrigin::Signed(1)),
				OriginCaller::system(frame_system::RawOrigin::Signed(2)),
			]);
			assert_err!(
				UtilityExt::batch_multi_origin(origin, items),
				Error::<Test>::OriginCountMismatch
			);
			assert_eq!(Balances::free_balance(1), 10);
		});
	}

	#[test]
	fn extension_passes_through_other_transactions() {
		new_test_ext().execute_with(|| {
			let ext = MultiOriginBatch::<Test>::new();
			let remark = RuntimeCall::System(SystemCall::remark { remark: vec![] });
			let batch = batch_call(vec![item(call_transfer(5, 3), SetOrigin::signed(1))]);
			let validate = |call: &RuntimeCall, origin: RuntimeOrigin| {
				ext.validate(
					origin,
					call,
					&call.get_dispatch_info(),
					0,
					(),
					&TxBaseImplication(()),
					TransactionSource::External,
				)
				.unwrap()
			};

			// Not a `batch_multi_origin` call: pass through, whatever the origin.
			assert_eq!(ext.weight(&remark), Weight::zero());
			let (_, val, origin) = validate(&remark, RuntimeOrigin::signed(1));
			assert!(matches!(val, Val::NotUsing));
			assert_eq!(origin.caller(), RuntimeOrigin::signed(1).caller());
			let (_, val, origin) = validate(&remark, RuntimeOrigin::none());
			assert!(matches!(val, Val::NotUsing));
			assert_eq!(origin.caller(), RuntimeOrigin::none().caller());

			// A `batch_multi_origin` call already authorized by someone else: pass through, refund.
			let (_, val, origin) = validate(&batch, RuntimeOrigin::signed(1));
			assert!(matches!(val, Val::NotUsing));
			assert_eq!(origin.caller(), RuntimeOrigin::signed(1).caller());
			let info = batch.get_dispatch_info();
			let pre = ext.clone().prepare(val, &origin, &batch, &info, 0).unwrap();
			assert_eq!(
				MultiOriginBatch::<Test>::post_dispatch_details(
					pre,
					&info,
					&Default::default(),
					0,
					&Ok(())
				),
				Ok(ext.weight(&batch))
			);

			// A general `batch_multi_origin` transaction: authorized, item pipelines charged twice.
			assert_eq!(
				ext.weight(&batch),
				SET_ORIGIN_WEIGHT
					.saturating_add(frame_system::CheckNonce::<Test>::from(0).weight(&remark))
					.saturating_mul(2)
			);
			let (_, val, origin) = validate(&batch, RuntimeOrigin::none());
			assert!(matches!(val, Val::Using));
			assert_eq!(origin.caller(), &OriginCaller::UtilityExt(Origin::MultiOriginBatch));
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn weight_is_refunded() {
		new_test_ext().execute_with(|| {
			let start = Weight::from_parts(100, 0);
			let end = Weight::from_parts(60, 0);
			let foobar = |_| {
				RuntimeCall::Example(ExampleCall::foobar {
					err: false,
					start_weight: start,
					end_weight: Some(end),
				})
			};
			let call = batch_call(vec![
				item(foobar(()), SetOrigin::signed(1)),
				item(foobar(()), SetOrigin::signed(2)),
			]);
			let (xt, info, len) = check(general_tx(call));
			let post_info = xt.apply::<Test>(&info, len).unwrap().unwrap();
			// The whole declared weight minus what the item calls did not use minus what the
			// item extensions did not use.
			let expected = info
				.total_weight()
				.saturating_sub((start - end).saturating_mul(2))
				.saturating_sub(SET_ORIGIN_UNSPENT.saturating_mul(2));
			assert_eq!(post_info.actual_weight, Some(expected));
		});
	}

	#[test]
	fn duplicated_item_is_rejected() {
		new_test_ext().execute_with(|| {
			let one = item(call_transfer(5, 3), SetOrigin::signed(1));
			let call = batch_call(vec![one.clone(), one]);
			assert_eq!(validate(call.clone()), Err(InvalidTransaction::Stale.into()));
			assert_eq!(apply(call).unwrap_err(), InvalidTransaction::Stale.into());
			assert_eq!(Balances::free_balance(1), 10);
			assert_eq!(Balances::free_balance(5), 2);
			assert_eq!(System::account_nonce(1), 0);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn items_of_one_signer_apply_in_sequence() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![
				item_with_nonce(call_transfer(5, 1), SetOrigin::signed(1), 0),
				item_with_nonce(call_transfer(5, 2), SetOrigin::signed(1), 1),
				item_with_nonce(call_transfer(5, 3), SetOrigin::signed(2), 0),
			]);
			let validity = validate(call.clone()).unwrap();
			assert!(validity.requires.is_empty());
			assert_eq!(validity.provides.len(), 3);

			assert_ok!(apply(call).unwrap());
			assert_eq!(Balances::free_balance(1), 7);
			assert_eq!(Balances::free_balance(2), 7);
			assert_eq!(Balances::free_balance(5), 8);
			assert_eq!(System::account_nonce(1), 2);
			assert_eq!(System::account_nonce(2), 1);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn validation_leaves_no_trace() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![
				item(call_transfer(5, 3), SetOrigin::signed(1)),
				item(call_transfer(5, 4), SetOrigin::signed(2)),
			]);
			assert_ok!(validate(call));
			assert_eq!(SetOrigin::prepared(), 0);
			assert_eq!(System::account_nonce(1), 0);
			assert_eq!(System::account_nonce(2), 0);
			assert_eq!(Balances::free_balance(1), 10);
			assert_eq!(Balances::free_balance(2), 10);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn item_origin_is_enforced() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![
				expecting(item(call_transfer(5, 3), SetOrigin::signed(1)), 1),
				expecting(item(call_transfer(5, 4), SetOrigin::signed(2)), 2),
			]);
			assert_ok!(apply(call).unwrap());
			assert_eq!(Balances::free_balance(5), 9);

			// Account 3 fills the slot expected to be account 2's.
			let call = batch_call(vec![
				expecting(item(call_transfer(5, 3), SetOrigin::signed(1)), 1),
				expecting(item(call_transfer(5, 4), SetOrigin::signed(3)), 2),
			]);
			assert_eq!(validate(call.clone()), Err(InvalidTransaction::BadSigner.into()));
			assert_eq!(apply(call).unwrap_err(), InvalidTransaction::BadSigner.into());
			assert_eq!(Balances::free_balance(5), 9);
			assert_eq!(System::account_nonce(3), 0);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn batch_priority_is_the_lowest_item_priority() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![
				item(call_transfer(5, 3), SetOrigin::signed(3)),
				item(call_transfer(5, 4), SetOrigin::signed(1)),
				item(call_transfer(5, 1), SetOrigin::signed(2)),
			]);
			assert_eq!(validate(call).unwrap().priority, 1);
		});
	}

	#[test]
	fn item_origin_can_be_any_pallets_origin() {
		new_test_ext().execute_with(|| {
			let council_only = RuntimeCall::Example(ExampleCall::council_only {});
			let members = |n| OriginCaller::Council(pallet_collective::RawOrigin::Members(n, 3));
			let mut item = item(council_only, SetOrigin::council());

			item.origin = members(3);
			assert_ok!(apply(batch_call(vec![item.clone()])).unwrap());
			assert_eq!(example::CouncilCalls::<Test>::get(), 1);

			item.origin = members(2);
			assert_eq!(
				apply(batch_call(vec![item])).unwrap_err(),
				InvalidTransaction::BadSigner.into()
			);
			assert_eq!(example::CouncilCalls::<Test>::get(), 1);
		});
	}

	#[test]
	fn signed_batch_transaction_fails_with_bad_origin_and_refunds_the_extension() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![item(call_transfer(5, 3), SetOrigin::signed(1))]);
			let ext: TxExtension = (MultiOriginBatch::new(), frame_system::CheckWeight::new());
			let payload = (&call, &ext, ext.implicit().unwrap()).encode();
			let payload = if payload.len() > 256 {
				sp_io::hashing::blake2_256(&payload).to_vec()
			} else {
				payload
			};
			let signature = sp_runtime::testing::TestSignature(1, payload);
			let uxt = UncheckedExtrinsic::new_signed(call.clone(), 1, signature, ext.clone());

			let (xt, info, len) = check(uxt);
			let result = xt.apply::<Test>(&info, len).unwrap();
			assert_eq!(result.unwrap_err().error, BadOrigin.into());
			// The weight declared for the item pipelines is refunded: nothing of it ran.
			let declared = MultiOriginBatch::<Test>::new().weight(&call);
			assert_ne!(declared, Weight::zero());
			assert_eq!(
				result.unwrap_err().post_info.actual_weight,
				Some(info.total_weight().saturating_sub(declared))
			);
			assert_eq!(Balances::free_balance(1), 10);
			assert_eq!(System::account_nonce(1), 0);
			assert_eq!(SetOrigin::prepared(), 0);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn pays_no_items_stay_free() {
		new_test_ext().execute_with(|| {
			let free = |err| RuntimeCall::Example(ExampleCall::free { err });

			let call = batch_call(vec![
				item(free(false), SetOrigin::signed(1)),
				item(call_transfer(5, 3), SetOrigin::signed(2)),
			]);
			assert_ok!(apply(call).unwrap());
			assert_eq!(SetOrigin::seen_pays_no(), 1);

			// Also when the batch fails: the item's dispatch info says so.
			let call = batch_call(vec![
				item(free(true), SetOrigin::signed(1)),
				item(call_transfer(5, 3), SetOrigin::signed(2)),
			]);
			assert!(apply(call).unwrap().is_err());
			assert_eq!(SetOrigin::seen_pays_no(), 2);
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn empty_batch_is_rejected() {
		new_test_ext().execute_with(|| {
			let call = batch_call(vec![]);
			assert_eq!(validate(call.clone()), Err(InvalidTransaction::Call.into()));
			assert_eq!(apply(call).unwrap_err(), InvalidTransaction::Call.into());
			assert_transient_storage_empty();
		});
	}

	#[test]
	fn items_pay_for_the_whole_transaction() {
		new_test_ext().execute_with(|| {
			let items: MultiOriginItemsOf<Test> = vec![
				item(call_transfer(5, 3), SetOrigin::signed(1)),
				item(call_transfer(5, 4), SetOrigin::signed(2)),
				item(call_transfer(5, 1), SetOrigin::signed(3)),
			]
			.try_into()
			.unwrap();
			let own = |item: &MultiOriginItemOf<Test>| {
				(
					item.call
						.get_dispatch_info()
						.call_weight
						.saturating_add(item.extension.weight(&item.call).saturating_mul(2)),
					item.encoded_size(),
				)
			};
			let (own_weight, own_len) =
				items.iter().fold((Weight::zero(), 0usize), |(w, l), item| {
					let (iw, il) = own(item);
					(w.saturating_add(iw), l.saturating_add(il))
				});
			// An overhead which does not split evenly.
			let overhead_weight = Weight::from_parts(10, 4);
			let overhead_len = 7;
			let info = DispatchInfo {
				call_weight: own_weight.saturating_add(overhead_weight),
				..Default::default()
			};

			let infos =
				Pallet::<Test>::multi_origin_item_infos(&items, &info, own_len + overhead_len);
			assert_eq!(infos.len(), 3);
			// The first item takes the remainder of the overhead.
			for (index, (item_info, item_len)) in infos.iter().enumerate() {
				let (iw, il) = own(&items[index]);
				let (share_w, share_l) = if index == 0 {
					(Weight::from_parts(4, 2), 3)
				} else {
					(Weight::from_parts(3, 1), 2)
				};
				assert_eq!(item_info.total_weight(), iw.saturating_add(share_w), "item {index}");
				assert_eq!(*item_len, il + share_l, "item {index}");
			}
			let (paid_weight, paid_len) =
				infos.iter().fold((Weight::zero(), 0usize), |(w, l), (i, il)| {
					(w.saturating_add(i.total_weight()), l + il)
				});
			assert_eq!(paid_weight, info.total_weight());
			assert_eq!(paid_len, own_len + overhead_len);
		});
	}
}
