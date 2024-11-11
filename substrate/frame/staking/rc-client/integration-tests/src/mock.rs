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

use frame_support::{derive_impl, parameter_types, traits::FindAuthor};
use pallet_session::{SessionHandler, SessionManager, ShouldEndSession};
use pallet_staking::{SessionInterface, UseNominatorsAndValidatorsMap, UseValidatorsMap};
use pallet_staking_rc_client::{AsyncBroker, AsyncOffenceBroker, AsyncSessionBroker};
use sp_core::{crypto::KeyTypeId, ConstU64};
use sp_runtime::{impl_opaque_keys, testing::UintAuthorityId};
use sp_staking::SessionIndex;

type Block = frame_system::mocking::MockBlock<Runtime>;
type BlockNumber = u64;
type AccountId = u64;
type Balance = u128;

pub const KEY_TYPE_IDS: KeyTypeId = KeyTypeId(*b"para");

/// Author of block is always 11
pub struct Author11;
impl FindAuthor<AccountId> for Author11 {
	fn find_author<'a, I>(_digests: I) -> Option<AccountId>
	where
		I: 'a + IntoIterator<Item = (frame_support::ConsensusEngineId, &'a [u8])>,
	{
		Some(11)
	}
}

frame_support::construct_runtime!(
	pub enum Runtime {
		// pallets that will be part of both relay-chain and parachain runtimes. Using the same in
		// the tests for simplicity.
		System: frame_system,
		Balances: pallet_balances,

		// relay-chain pallets.
		RCSession: pallet_session,
		RCHistorical: pallet_session::historical,
		RCAuthorship: pallet_authorship,

		// pallets in a different consensus system than relay-chain. The Client pallet is used as a
		// broker for the staking to communcate (one way, async) with the staking pallet.
		Staking: pallet_staking,
		Timestamp: pallet_timestamp,
		Client: pallet_staking_rc_client,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

parameter_types! {
	pub static ExistentialDeposit: Balance = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type MaxLocks = frame_support::traits::ConstU32<1024>;
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<5>;
	type WeightInfo = ();
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub dummy: UintAuthorityId,
	}
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = AccountId;
	type ValidatorIdOf = TransparentAccountConvertion;
	type ShouldEndSession = TestShouldEndSession;
	type NextSessionRotation = ();
	// implemented by the relay-chain side staking client pallet.
	type SessionManager = TestSessionManager;
	// implemented by the relay-chain side staking client pallet.
	type SessionHandler = TestSessionHandler;
	type Keys = SessionKeys;
	type WeightInfo = ();
}

impl pallet_session::historical::Config for Runtime {
	type FullIdentification = pallet_staking::Exposure<AccountId, Balance>;
	type FullIdentificationOf = pallet_staking::ExposureOf<Runtime>;
}
impl pallet_authorship::Config for Runtime {
	type FindAuthor = Author11;
	// implemented by the relay-chain side staking client pallet.
	type EventHandler = Staking;
}

pub struct TransparentAccountConvertion;
impl sp_runtime::traits::Convert<AccountId, Option<AccountId>> for TransparentAccountConvertion {
	fn convert(a: AccountId) -> Option<AccountId> {
		Some(a)
	}
}

pub struct TestShouldEndSession;
impl ShouldEndSession<BlockNumber> for TestShouldEndSession {
	fn should_end_session(_now: BlockNumber) -> bool {
		todo!()
	}
}

pub struct TestSessionManager;
impl SessionManager<AccountId> for TestSessionManager {
	fn new_session(_new_index: SessionIndex) -> Option<Vec<AccountId>> {
		todo!()
	}
	fn end_session(_end_index: SessionIndex) {
		todo!()
	}
	fn start_session(_start_index: SessionIndex) {
		todo!()
	}
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<AccountId>> {
		todo!()
	}
}

pub struct TestSessionHandler;
impl SessionHandler<AccountId> for TestSessionHandler {
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[KEY_TYPE_IDS];

	fn on_disabled(_validator_index: u32) {
		todo!()
	}
	fn on_new_session<Ks: sp_runtime::traits::OpaqueKeys>(
		_changed: bool,
		_validators: &[(AccountId, Ks)],
		_queued_validators: &[(AccountId, Ks)],
	) {
		todo!()
	}
	fn on_genesis_session<Ks: sp_runtime::traits::OpaqueKeys>(_validators: &[(AccountId, Ks)]) {
		todo!()
	}
	fn on_before_session_ending() {
		todo!()
	}
}

parameter_types! {
	pub static MaxWinners: u32 = 10;
}

#[derive_impl(pallet_staking::config_preludes::TestDefaultConfig)]
impl pallet_staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type UnixTime = Timestamp;
	type ElectionProvider =
		frame_election_provider_support::NoElection<(AccountId, BlockNumber, Staking, MaxWinners)>;
	type GenesisElectionProvider = Self::ElectionProvider;
	type AdminOrigin = frame_system::EnsureRoot<AccountId>;
	type EraPayout = ();
	type VoterList = UseNominatorsAndValidatorsMap<Self>;
	type TargetList = UseValidatorsMap<Self>;

	// session interfaces are implemented by the rc-client pallet.

	// session related types must live in the parachain (ie, rc-client acts as broker).
	type NextNewSession = Client;
	type SessionInterface = Client;
}

parameter_types! {
	pub static MaxOffenders: u32 = 10;
}

impl pallet_staking_rc_client::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = AccountId;
	type FullValidatorId = pallet_staking::Exposure<Self::AccountId, Balance>;
	type Staking = Staking;
	type MaxOffenders = MaxOffenders;
	type MaxValidatorSet = MaxWinners;
	type SessionKeys = SessionKeys;
	type RelayChainClient = Broker;
}

/// Type that implements the XCM communication between the rc-client and the relay chain.
pub struct Broker;
impl AsyncBroker for Broker {}

impl AsyncSessionBroker for Broker {
	type AccountId = AccountId;
	type SessionKeys = SessionKeys;
	type SessionKeysProof = pallet_staking_rc_client::SessionKeysProof;
	type MaxValidatorSet = MaxWinners;
	type Error = &'static str;

	fn set_session_keys(
		_who: Self::AccountId,
		_session_keys: Self::SessionKeys,
		_proof: Self::SessionKeysProof,
	) -> Result<(), Self::Error> {
		todo!()
	}

	fn purge_session_keys(_who: Self::AccountId) -> Result<(), Self::Error> {
		todo!()
	}

	fn new_validator_set(
		_session_index: SessionIndex,
		_validator_set: sp_runtime::BoundedVec<Self::AccountId, Self::MaxValidatorSet>,
	) -> Result<(), Self::Error> {
		todo!()
	}
}

impl SessionInterface<AccountId> for Broker {
	fn validators() -> Vec<AccountId> {
		todo!()
	}
	fn disable_validator(_validator_index: u32) -> bool {
		todo!()
	}
	fn prune_historical_up_to(_up_to: SessionIndex) {
		todo!()
	}
}

impl AsyncOffenceBroker for Broker {}

#[derive(Default)]
pub struct ExtBuilder {}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		use sp_runtime::BuildStorage;

		sp_tracing::try_init_simple();

		let mut storage =
			frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		// set pallet's genesis state
		sp_io::TestExternalities::from(storage)
	}

	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		sp_tracing::try_init_simple();

		let mut ext = self.build();
		ext.execute_with(test);
	}
}
