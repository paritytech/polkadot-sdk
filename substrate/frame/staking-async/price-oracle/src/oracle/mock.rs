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

//! Test utilities for pallet-price-oracle

use crate::{
	extension::{OnlyOracleAuthorities, SetPriorityFromProducedIn},
	oracle::{
		self as pallet_price_oracle,
		offchain::{Endpoint, Method, ParsingMethod},
		MomentOf, TallyOuterError,
	},
	tally::SimpleAverage,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{derive_impl, parameter_types, traits::Time};
use frame_system::pallet_prelude::BlockNumberFor;
use parking_lot::RwLock;
use scale_info::TypeInfo;
use sp_core::{
	offchain::{
		testing::{OffchainState, PoolState, TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	ConstU32, ConstU64,
};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{
	impl_opaque_keys,
	testing::{TestSignature, UintAuthorityId},
	traits::{Convert, IdentityLookup},
	BuildStorage, FixedU128, Percent, RuntimeAppPublic,
};
use std::sync::Arc;

pub type Extensions = (OnlyOracleAuthorities<Runtime>, SetPriorityFromProducedIn<Runtime>);
pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type T = Runtime;
pub type Extrinsic = sp_runtime::testing::TestXt<RuntimeCall, Extensions>;
pub type AssetId = <Runtime as pallet_price_oracle::Config>::AssetId;
pub type BlockNumber = BlockNumberFor<Runtime>;
pub type Moment = MomentOf<Runtime>;
pub type AccountId = <Runtime as frame_system::Config>::AccountId;

// Simple u64-based AppCrypto for testing. Wraps a u64 so that different authorities
// can be distinguished (e.g. `TestAuthId(5)` maps to `AccountId = 5`).
#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	codec::DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct TestAuthId(pub u64);

impl RuntimeAppPublic for TestAuthId {
	const ID: sp_core::crypto::KeyTypeId = sp_core::crypto::KeyTypeId(*b"test");

	type Signature = TestSignature;
	type ProofOfPossession = TestSignature;

	fn all() -> Vec<Self> {
		vec![]
	}

	fn generate_pair(_: Option<Vec<u8>>) -> Self {
		TestAuthId(0)
	}

	fn sign<M: AsRef<[u8]>>(&self, _msg: &M) -> Option<Self::Signature> {
		None
	}

	fn verify<M: AsRef<[u8]>>(&self, _msg: &M, _signature: &Self::Signature) -> bool {
		true
	}

	fn generate_proof_of_possession(&mut self, _owner: &[u8]) -> Option<Self::Signature> {
		None
	}

	fn verify_proof_of_possession(&self, _owner: &[u8], _pop: &Self::Signature) -> bool {
		true
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		self.0.to_le_bytes().to_vec()
	}
}

impl AsRef<[u8]> for TestAuthId {
	fn as_ref(&self) -> &[u8] {
		&[]
	}
}

impl frame_system::offchain::AppCrypto<UintAuthorityId, TestSignature> for TestAuthId {
	type RuntimeAppPublic = UintAuthorityId;
	type GenericPublic = UintAuthorityId;
	type GenericSignature = TestSignature;
}

impl<LocalCall> frame_system::offchain::CreateTransactionBase<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type RuntimeCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateBare<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
		Extrinsic::new_bare(call)
	}
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = UintAuthorityId;
	type Signature = TestSignature;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_signed_transaction<
		C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>,
	>(
		call: RuntimeCall,
		_public: Self::Public,
		account: AccountId,
		// TODO: maybe use and add check-nonce to our e2e test
		_nonce: u32,
	) -> Option<Extrinsic> {
		let extensions = (
			OnlyOracleAuthorities::<Runtime>::default(),
			SetPriorityFromProducedIn::<Runtime>::default(),
		);
		Some(Extrinsic::new_signed(call, account.into(), (), extensions))
	}
}

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		PriceOracle: pallet_price_oracle,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = ();
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub price_oracle: TestAuthId,
	}
}

parameter_types! {
	pub static PriceUpdates: Vec<(u32, pallet_price_oracle::PriceDataOf<T>)> = Default::default();
}

pub struct OnPriceUpdate;
impl pallet_price_oracle::OnPriceUpdate<AssetId, BlockNumber, Moment> for OnPriceUpdate {
	fn on_price_update(
		asset_id: AssetId,
		new: pallet_price_oracle::PriceData<BlockNumber, Moment>,
	) {
		PriceUpdates::mutate(|updates| updates.push((asset_id, new)));
	}
}

parameter_types! {
	pub static PriceUpdateInterval: u64 = 5;
	pub static HistoryDepth: u32 = 4;
	pub static MaxVotesPerBlock: u32 = 8;
	pub static MaxVoteAge: u64 = 4;
	pub static NextTallyFails: Option<TallyOuterError<()>> = None;
	pub static Authorities: Option<Vec<(u64, Percent)>> = None;
	pub static MaxAuthorities: u32 = 4;
}

pub struct TestTally;
impl pallet_price_oracle::Tally for TestTally {
	type AssetId = AssetId;
	type AccountId = AccountId;
	type BlockNumber = BlockNumber;
	type Error = ();

	fn tally(
		asset_id: Self::AssetId,
		votes: Vec<(Self::AccountId, FixedU128, BlockNumber)>,
	) -> Result<(FixedU128, Percent), TallyOuterError<Self::Error>> {
		if let Some(err) = NextTallyFails::take() {
			Err(err)
		} else {
			SimpleAverage::<T>::tally(asset_id, votes)
		}
	}
}

pub struct TimeProvider;
impl Time for TimeProvider {
	type Moment = u64;
	fn now() -> Self::Moment {
		(System::block_number() * 1000) as u64
	}
}

pub struct TestAuthorityIdToAccountId;
impl Convert<TestAuthId, AccountId> for TestAuthorityIdToAccountId {
	fn convert(id: TestAuthId) -> AccountId {
		id.0
	}
}

impl pallet_price_oracle::Config for Runtime {
	type AuthorityId = TestAuthId;
	type AuthorityIdToAccountId = TestAuthorityIdToAccountId;
	type PriceUpdateInterval = PriceUpdateInterval;
	type AssetId = u32;
	type HistoryDepth = HistoryDepth;
	type MaxAuthorities = MaxAuthorities;
	type MaxEndpointsPerAsset = ConstU32<8>;
	type MaxVotesPerBlock = MaxVotesPerBlock;
	type MaxVoteAge = MaxVoteAge;
	type TallyManager = TestTally;
	// Note: relay and para-block is the same in tests.
	type RelayBlockNumberProvider = System;
	type TimeProvider = TimeProvider;
	type OnPriceUpdate = OnPriceUpdate;
	type DefaultRequestDeadline = ConstU64<2000>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
	type WeightInfo = ();
}

pub struct ExtBuilder {
	assets: Vec<(AssetId, Vec<Endpoint>)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			assets: vec![(
				1,
				vec![Endpoint {
					body: Default::default(),
					headers: Default::default(),
					deadline: None,
					confidence: Default::default(),
					method: Method::Get,
					parsing_method: ParsingMethod::CryptoCompareFree,
					requires_api_key: false,
					url: "ocw.local.io/price".to_string().into_bytes().try_into().unwrap(),
				}],
			)],
		}
	}
}

impl ExtBuilder {
	pub fn extra_asset(mut self, id: AssetId, endpoints: Vec<Endpoint>) -> Self {
		self.assets.push((id, endpoints));
		self
	}

	pub fn only_asset(mut self, id: AssetId, endpoints: Vec<Endpoint>) -> Self {
		self.assets = vec![(id, endpoints)];
		self
	}

	pub fn max_votes_per_block(self, max: u32) -> Self {
		MaxVotesPerBlock::set(max);
		self
	}

	pub fn history_depth(self, depth: u32) -> Self {
		HistoryDepth::set(depth);
		self
	}

	pub fn price_update_interval(self, interval: u64) -> Self {
		PriceUpdateInterval::set(interval);
		self
	}

	pub fn max_authorities(self, max: u32) -> Self {
		MaxAuthorities::set(max);
		self
	}

	pub fn empty(mut self) -> Self {
		self.assets = vec![];
		self
	}

	pub fn authorities(self, count: u32) -> Self {
		Authorities::set(Some(
			(1..=count as u64).map(|i| (i, Percent::from_percent(100))).collect(),
		));
		self
	}
}

impl ExtBuilder {
	pub(crate) fn build(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let mut storage =
			frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		// default authorities for all tests.
		let authorities = Authorities::take().unwrap_or_else(|| {
			vec![
				(1, Percent::from_percent(100)),
				(2, Percent::from_percent(100)),
				(3, Percent::from_percent(100)),
				(4, Percent::from_percent(100)),
			]
		});

		let _ = pallet_price_oracle::GenesisConfig::<Runtime> {
			maybe_authorities: Some(authorities),
			tracked_assets: self.assets,
		}
		.assimilate_storage(&mut storage);

		let mut ext = sp_io::TestExternalities::from(storage);
		ext.execute_with(|| System::set_block_number(7));
		ext
	}

	pub(crate) fn build_offchainify(
		self,
	) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>, Arc<RwLock<OffchainState>>) {
		// set keys for 1 account that is actually an authority.
		UintAuthorityId::set_all_keys(vec![1]);

		let mut ext = self.build();
		let (offchain, offchain_state) = TestOffchainExt::new();
		let (pool, pool_state) = TestTransactionPoolExt::new();

		ext.register_extension(OffchainDbExt::new(offchain.clone()));
		ext.register_extension(OffchainWorkerExt::new(offchain));
		ext.register_extension(TransactionPoolExt::new(pool));
		ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));

		(ext, pool_state, offchain_state)
	}

	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		let mut ext = self.build();
		ext.execute_with(test);
		ext.execute_with(|| PriceOracle::do_try_state(System::block_number()).unwrap());
	}

	pub fn build_offchain_and_execute(
		self,
		test: impl FnOnce(Arc<RwLock<PoolState>>, Arc<RwLock<OffchainState>>) -> (),
	) {
		let (mut ext, pool_state, offchain_state) = self.build_offchainify();
		ext.execute_with(|| test(pool_state, offchain_state));
		ext.execute_with(|| PriceOracle::do_try_state(System::block_number()).unwrap());
	}
}

/// Bump block number, and force the caller to name the next expected block.
///
/// This is useful to keep tests readable by writing `bump_block_number(42)`, making it clear that
/// we bumped the block, and the new one is 42.
///
/// Using `bump_block_number(System::block_number() + 1)` is cheating :)
pub fn bump_block_number(next: BlockNumber) {
	frame_system::Pallet::<T>::set_block_number(frame_system::Pallet::<T>::block_number() + 1);
	assert_eq!(next, System::block_number(), "next expected block number is not guessed correctly");
}

parameter_types! {
	pub static OracleEventIndex: usize = 0;
}

/// Read all events from the oracle pallet since the last time this was called.
pub fn oracle_events_since_last_call() -> Vec<pallet_price_oracle::Event<Runtime>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.filter_map(
			|r| if let RuntimeEvent::PriceOracle(inner) = r.event { Some(inner) } else { None },
		)
		.collect();
	let seen = OracleEventIndex::get();
	OracleEventIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}
