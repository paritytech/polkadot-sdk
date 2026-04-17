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

use crate as pallet_pgas_allowance;

use frame_support::{
	derive_impl,
	dispatch::DispatchClass,
	pallet_prelude::Get,
	parameter_types,
	traits::{AsEnsureOriginWithArg, ConstU64, Contains},
	weights::{Weight, WeightToFee as WeightToFeeT},
};
use frame_system::EnsureRoot;
use pallet_transaction_payment::FungibleAdapter;
use sp_runtime::{traits::SaturatedConversion, BuildStorage};

pub type AccountId = u64;
pub type Balance = u64;
pub type AssetId = u32;

type Block = frame_system::mocking::MockBlock<Runtime>;

pub const PGAS_ASSET_ID: AssetId = 42;
pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const CHARLIE: AccountId = 3;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,
		TransactionPayment: pallet_transaction_payment,
		Assets: pallet_assets,
		PgasAllowance: pallet_pgas_allowance,
		DummyPallet: pallet_dummy,
	}
);

parameter_types! {
	pub(crate) static ExtrinsicBaseWeight: Weight = Weight::zero();
}

pub struct BlockWeights;
impl Get<frame_system::limits::BlockWeights> for BlockWeights {
	fn get() -> frame_system::limits::BlockWeights {
		frame_system::limits::BlockWeights::builder()
			.base_block(Weight::zero())
			.for_class(DispatchClass::all(), |weights| {
				weights.base_extrinsic = ExtrinsicBaseWeight::get().into();
			})
			.for_class(DispatchClass::non_mandatory(), |weights| {
				weights.max_total = Weight::from_parts(1024, u64::MAX).into();
			})
			.build_or_panic()
	}
}

parameter_types! {
	pub static WeightToFee: u64 = 1;
	pub static TransactionByteFee: u64 = 1;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type BlockWeights = BlockWeights;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
}

impl WeightToFeeT for WeightToFee {
	type Balance = Balance;

	fn weight_to_fee(weight: &Weight) -> Self::Balance {
		Self::Balance::saturated_from(weight.ref_time())
			.saturating_mul(WEIGHT_TO_FEE.with(|v| *v.borrow()))
	}
}

impl WeightToFeeT for TransactionByteFee {
	type Balance = Balance;

	fn weight_to_fee(weight: &Weight) -> Self::Balance {
		Self::Balance::saturated_from(weight.ref_time())
			.saturating_mul(TRANSACTION_BYTE_FEE.with(|v| *v.borrow()))
	}
}

#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type WeightToFee = WeightToFee;
	type LengthToFee = TransactionByteFee;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config for Runtime {
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	// No deposits in tests.
	type AssetDeposit = ConstU64<0>;
	type AssetAccountDeposit = ConstU64<0>;
	type MetadataDepositBase = ConstU64<0>;
	type MetadataDepositPerByte = ConstU64<0>;
	type ApprovalDeposit = ConstU64<0>;
}

parameter_types! {
	pub const PGASAssetId: AssetId = PGAS_ASSET_ID;
}

/// Filter that only matches the dummy pallet's calls; everything else falls through.
pub struct PGASCallFilter;
impl Contains<RuntimeCall> for PGASCallFilter {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(call, RuntimeCall::DummyPallet(..))
	}
}

impl pallet_pgas_allowance::Config for Runtime {
	type AssetId = AssetId;
	type Assets = Assets;
	type PGASAssetId = PGASAssetId;
	type CallFilter = PGASCallFilter;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchmarkHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_pgas_allowance::BenchmarkHelperTrait<AccountId, AssetId, Balance> for BenchmarkHelper {
	fn mint_pgas(who: &AccountId, asset_id: AssetId, amount: Balance) {
		use frame_support::traits::tokens::fungibles::Mutate;
		<Assets as Mutate<AccountId>>::mint_into(asset_id, who, amount).unwrap();
	}
}

#[frame_support::pallet(dev_mode)]
pub mod pallet_dummy {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A no-op call whose only purpose is to match the PGAS filter in tests.
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}
}

impl pallet_dummy::Config for Runtime {}

pub struct ExtBuilder {
	native_balances: Vec<(AccountId, Balance)>,
	pgas_balances: Vec<(AccountId, Balance)>,
	base_weight: Weight,
	byte_fee: u64,
	weight_to_fee: u64,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			native_balances: vec![],
			pgas_balances: vec![],
			base_weight: Weight::zero(),
			byte_fee: 1,
			weight_to_fee: 1,
		}
	}
}

impl ExtBuilder {
	pub fn with_native(mut self, balances: Vec<(AccountId, Balance)>) -> Self {
		self.native_balances = balances;
		self
	}

	pub fn with_pgas(mut self, balances: Vec<(AccountId, Balance)>) -> Self {
		self.pgas_balances = balances;
		self
	}

	pub fn base_weight(mut self, base_weight: Weight) -> Self {
		self.base_weight = base_weight;
		self
	}

	fn set_constants(&self) {
		ExtrinsicBaseWeight::mutate(|v| *v = self.base_weight);
		TRANSACTION_BYTE_FEE.with(|v| *v.borrow_mut() = self.byte_fee);
		WEIGHT_TO_FEE.with(|v| *v.borrow_mut() = self.weight_to_fee);
	}

	pub fn build(self) -> sp_io::TestExternalities {
		self.set_constants();
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.native_balances.clone(),
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		pallet_pgas_allowance::GenesisConfig::<Runtime> {
			min_balance: 1,
			_phantom: Default::default(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext: sp_io::TestExternalities = t.into();
		ext.execute_with(|| {
			System::set_block_number(1);
			for (who, bal) in &self.pgas_balances {
				use frame_support::traits::tokens::fungibles::Mutate;
				<Assets as Mutate<AccountId>>::mint_into(PGAS_ASSET_ID, who, *bal).unwrap();
			}
		});
		ext
	}
}

/// Build a `DispatchInfo` with the given call weight.
pub fn info_from_weight(w: Weight) -> frame_support::dispatch::DispatchInfo {
	frame_support::dispatch::DispatchInfo { call_weight: w, ..Default::default() }
}

/// Build a `PostDispatchInfo` reporting the given actual weight.
pub fn post_info_from_weight(w: Weight) -> frame_support::dispatch::PostDispatchInfo {
	frame_support::dispatch::PostDispatchInfo {
		actual_weight: Some(w),
		pays_fee: Default::default(),
	}
}

pub fn default_post_info() -> frame_support::dispatch::PostDispatchInfo {
	frame_support::dispatch::PostDispatchInfo { actual_weight: None, pays_fee: Default::default() }
}
