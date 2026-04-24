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
	traits::{AsEnsureOriginWithArg, ConstU64, Contains, Get},
	unsigned::TransactionValidityError,
	weights::{IdentityFee, Weight},
};
use frame_system::EnsureRoot;
use pallet_asset_conversion_tx_payment::OnChargeAssetTransaction;
use pallet_transaction_payment::FungibleAdapter;
use sp_runtime::{
	BuildStorage,
	traits::{DispatchInfoOf, PostDispatchInfoOf},
	transaction_validity::InvalidTransaction,
};

pub type AccountId = <Runtime as frame_system::Config>::AccountId;
pub type Balance = <Runtime as pallet_balances::Config>::Balance;
pub type AssetId = <Runtime as pallet_assets::Config>::AssetId;

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
		AssetTxPayment: pallet_asset_conversion_tx_payment,
		PgasAllowance: pallet_pgas_allowance,
	}
);

pub struct BlockWeights;
impl Get<frame_system::limits::BlockWeights> for BlockWeights {
	fn get() -> frame_system::limits::BlockWeights {
		frame_system::limits::BlockWeights::builder()
			.base_block(Weight::zero())
			.for_class(DispatchClass::all(), |weights| {
				weights.base_extrinsic = Weight::zero();
			})
			.for_class(DispatchClass::non_mandatory(), |weights| {
				weights.max_total = Weight::from_parts(1024, u64::MAX).into();
			})
			.build_or_panic()
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type BlockWeights = BlockWeights;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
}

#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = IdentityFee<Balance>;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config for Runtime {
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
}

pub struct PgasId;
impl Get<AssetId> for PgasId {
	fn get() -> AssetId {
		PGAS_ASSET_ID
	}
}

/// `OnChargeAssetTransaction` that rejects anything it's handed. The mock only exercises the PGAS
/// asset path, so the delegate is never reached in tests; a reject keeps the surface minimal.
pub struct RejectOtherAssets;
impl OnChargeAssetTransaction<Runtime> for RejectOtherAssets {
	type AssetId = AssetId;
	type Balance = Balance;
	type LiquidityInfo = ();

	fn withdraw_fee(
		_who: &AccountId,
		_call: &RuntimeCall,
		_info: &DispatchInfoOf<RuntimeCall>,
		_asset_id: AssetId,
		_fee: Balance,
		_tip: Balance,
	) -> Result<(), TransactionValidityError> {
		Err(InvalidTransaction::Payment.into())
	}

	fn can_withdraw_fee(
		_who: &AccountId,
		_asset_id: AssetId,
		_fee: Balance,
	) -> Result<(), TransactionValidityError> {
		Err(InvalidTransaction::Payment.into())
	}

	fn correct_and_deposit_fee(
		_who: &AccountId,
		_info: &DispatchInfoOf<RuntimeCall>,
		_post_info: &PostDispatchInfoOf<RuntimeCall>,
		_corrected_fee: Balance,
		_tip: Balance,
		_asset_id: AssetId,
		_already_withdrawn: (),
	) -> Result<Balance, TransactionValidityError> {
		Err(InvalidTransaction::Payment.into())
	}
}

impl pallet_asset_conversion_tx_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AssetId = AssetId;
	type OnChargeAssetTransaction =
		pallet_pgas_allowance::PgasOnChargeAssetTransaction<PgasId, Assets, RejectOtherAssets>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = AssetTxPaymentBenchmarkHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct AssetTxPaymentBenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_asset_conversion_tx_payment::BenchmarkHelperTrait<AccountId, AssetId, AssetId>
	for AssetTxPaymentBenchmarkHelper
{
	fn create_asset_id_parameter(id: u32) -> (AssetId, AssetId) {
		(id, id)
	}
	fn setup_balances_and_pool(_asset_id: AssetId, _account: AccountId) {
		// The PGAS benchmarks never swap, so pool setup is unnecessary.
	}
}

/// Matches `frame_system` calls so the filter matches test-supplied `System::remark` calls.
pub struct PGASCallFilter;
impl Contains<RuntimeCall> for PGASCallFilter {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(call, RuntimeCall::System(..))
	}
}

impl pallet_pgas_allowance::Config for Runtime {
	type PgasId = PgasId;
	type CallFilter = PGASCallFilter;
	type Fungibles = Assets;
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

#[derive(Default)]
pub struct ExtBuilder {
	native_balances: Vec<(AccountId, Balance)>,
	pgas_balances: Vec<(AccountId, Balance)>,
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

	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.native_balances.clone(),
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		pallet_assets::GenesisConfig::<Runtime> {
			assets: vec![(PGAS_ASSET_ID, ALICE, true, 1)],
			accounts: self
				.pgas_balances
				.iter()
				.map(|(who, bal)| (PGAS_ASSET_ID, *who, *bal))
				.collect(),
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext: sp_io::TestExternalities = t.into();
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}

/// Build a `DispatchInfo` with the given call weight.
pub fn info_from_weight(
	w: frame_support::weights::Weight,
) -> frame_support::dispatch::DispatchInfo {
	frame_support::dispatch::DispatchInfo { call_weight: w, ..Default::default() }
}

/// Build a `PostDispatchInfo` reporting the given actual weight.
pub fn post_info_from_weight(
	w: frame_support::weights::Weight,
) -> frame_support::dispatch::PostDispatchInfo {
	frame_support::dispatch::PostDispatchInfo {
		actual_weight: Some(w),
		pays_fee: Default::default(),
	}
}

pub fn default_post_info() -> frame_support::dispatch::PostDispatchInfo {
	frame_support::dispatch::PostDispatchInfo { actual_weight: None, pays_fee: Default::default() }
}
