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

use crate::{self as pallet_rc_client, *};

use frame_support::{derive_impl, parameter_types};
use sp_runtime::{impl_opaque_keys, testing::UintAuthorityId, Perbill};
use sp_staking::{offence::OnOffenceHandler, SessionIndex};

use core::marker::PhantomData;
use std::collections::BTreeMap;

type Block = frame_system::mocking::MockBlock<Test>;

type AccountId = u64;
type AuthorshipPoints = u32;
type Weight = sp_runtime::Weight;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Client: pallet_rc_client,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Block = Block;
}

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub dummy: UintAuthorityId,
	}
}

impl From<UintAuthorityId> for MockSessionKeys {
	fn from(dummy: UintAuthorityId) -> Self {
		Self { dummy }
	}
}

parameter_types! {
	pub static MaxOffenders: u32 = 5;
	pub static MaxValidatorSet: u32 = 10;
}

impl crate::pallet::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Staking = MockStaking<Test>;
	type ValidatorId = AccountId;
	type FullValidatorId = pallet_staking::Exposure<Self::AccountId, u64>;
	type MaxOffenders = MaxOffenders;
	type MaxValidatorSet = MaxValidatorSet;
	type SessionKeys = MockSessionKeys;
	type RelayChainClient = MockRelayClient;
}

parameter_types! {
	// Stores receved messages from `MockRelayClient` for testing.
	pub static Outbound: Vec<MockMessages> = vec![];
}

#[derive(Clone, PartialEq, Debug)]
pub enum MockMessages {
	SetSessionKeys((AccountId, MockSessionKeys, SessionKeysProof)),
}

pub(crate) struct MockRelay;
impl MockRelay {
	fn send_it(msg: MockMessages) {
		Outbound::mutate(|o| {
			o.push(msg);
		})
	}
}

pub struct MockRelayClient;
impl AsyncBroker for MockRelayClient {}

impl AsyncSessionBroker for MockRelayClient {
	type AccountId = AccountId;
	type SessionKeys = MockSessionKeys;
	type SessionKeysProof = SessionKeysProof;
	type MaxValidatorSet = MaxValidatorSet;
	type Error = &'static str;

	fn set_session_keys(
		who: Self::AccountId,
		session_keys: Self::SessionKeys,
		proof: Self::SessionKeysProof,
	) -> Result<(), Self::Error> {
		MockRelay::send_it(MockMessages::SetSessionKeys((who, session_keys, proof)));
		Ok(())
	}
	fn purge_session_keys(_who: Self::AccountId) -> Result<(), Self::Error> {
		// TODO: build XCM and "send" ut to MockRelay
		todo!()
	}
	fn new_validator_set(
		_session_index: SessionIndex,
		_validator_set: sp_runtime::BoundedVec<Self::AccountId, Self::MaxValidatorSet>,
	) -> Result<(), Self::Error> {
		// TODO: build XCM and "send" ut to MockRelay
		todo!()
	}
}

impl AsyncOffenceBroker for MockRelayClient {}

impl<AccountId> SessionInterface<AccountId> for MockRelayClient
where
	Client: SessionInterface<AccountId>,
{
	fn validators() -> Vec<AccountId> {
		<Client as SessionInterface<AccountId>>::validators()
	}
	fn disable_validator(validator_index: u32) -> bool {
		<Client as SessionInterface<AccountId>>::disable_validator(validator_index)
	}
	fn prune_historical_up_to(up_to: SessionIndex) {
		<Client as SessionInterface<AccountId>>::prune_historical_up_to(up_to)
	}
}

parameter_types! {
	pub static AuthoringState: Vec<(AccountId, SessionIndex, AuthorshipPoints)> = vec![];
	pub static OffencesState: BTreeMap<SessionIndex, (AccountId, AccountId, Perbill)> = BTreeMap::new();
}

pub struct MockStaking<T>(PhantomData<T>);
impl<T: Config> SessionManager<T::AccountId> for MockStaking<T> {
	fn new_session(_new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		todo!()
	}
	fn end_session(_end_index: SessionIndex) {
		todo!()
	}
	fn start_session(_start_index: SessionIndex) {
		todo!()
	}
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		todo!()
	}
}

impl<T: Config> AuthorshipEventHandler<AccountId, BlockNumberFor<T>> for MockStaking<T> {
	fn note_author(author: AccountId) {
		// one point per authoring.
		AuthoringState::mutate(|s| {
			let session = 0; // TODO
			s.push((author, session, 1))
		});
	}
}

impl<T: Config> OnOffenceHandler<T::AccountId, OffenderOf<T>, Weight> for MockStaking<T> {
	fn on_offence(
		_offenders: &[sp_staking::offence::OffenceDetails<T::AccountId, OffenderOf<T>>],
		_slash_fraction: &[Perbill],
		_session: SessionIndex,
	) -> Weight {
		todo!()
	}
}

#[derive(Default)]
pub struct ExtBuilder {}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		use sp_runtime::BuildStorage;

		sp_tracing::try_init_simple();

		let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		// set pallet's genesis state
		sp_io::TestExternalities::from(storage)
	}

	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		sp_tracing::try_init_simple();

		let mut ext = self.build();
		ext.execute_with(test);
	}
}
