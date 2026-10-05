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

//! Mock runtime for pallet-staking-async-ah-client tests.

use crate::*;
use alloc::vec::Vec;
use frame_support::{derive_impl, parameter_types, weights::Weight};
use sp_runtime::{traits::OpaqueKeys, BuildStorage, DispatchError, KeyTypeId, Perbill};
use sp_staking::offence::{OffenceSeverity, OnOffenceHandler};

/// Mock session keys for testing.
#[derive(Clone, PartialEq, Eq, Debug, codec::Encode, codec::Decode, scale_info::TypeInfo)]
pub struct MockSessionKeys {
	pub dummy: [u8; 32],
}

const MOCK_KEY_TYPE: KeyTypeId = KeyTypeId(*b"mock");

impl OpaqueKeys for MockSessionKeys {
	type KeyTypeIdProviders = ();

	fn key_ids() -> &'static [KeyTypeId] {
		&[MOCK_KEY_TYPE]
	}

	fn get_raw(&self, _: KeyTypeId) -> &[u8] {
		&self.dummy
	}

	fn ownership_proof_is_valid(&self, _: &[u8], _: &[u8]) -> bool {
		true
	}
}

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		StakingAsyncAhClient: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = ();
}

pub struct MockSessionInterface;
impl SessionInterface for MockSessionInterface {
	type ValidatorId = u64;
	type AccountId = u64;
	type Keys = MockSessionKeys;

	fn validators() -> Vec<Self::ValidatorId> {
		vec![1, 2, 3]
	}
	fn prune_up_to(_up_to: u32) {}
	fn report_offence(_offender: Self::ValidatorId, _severity: OffenceSeverity) {}
	fn set_keys(account: &Self::AccountId, keys: Self::Keys) -> DispatchResult {
		if let Some(err) = SetKeysError::get() {
			return Err(err);
		}
		SetKeysCalls::mutate(|calls| calls.push((*account, keys)));
		RegisteredKeys::mutate(|registered| {
			if !registered.contains(account) {
				registered.push(*account)
			}
		});
		Ok(())
	}
	fn purge_keys(account: &Self::AccountId) -> DispatchResult {
		if let Some(err) = PurgeKeysError::get() {
			return Err(err);
		}
		PurgeKeysCalls::mutate(|calls| calls.push(*account));
		RegisteredKeys::mutate(|registered| registered.retain(|a| a != account));
		Ok(())
	}
	fn set_keys_weight() -> Weight {
		Weight::zero()
	}
	fn purge_keys_weight() -> Weight {
		Weight::zero()
	}
}

impl frame_support::traits::ValidatorRegistration<u64> for MockSessionInterface {
	fn is_registered(id: &u64) -> bool {
		RegisteredKeys::get().contains(id)
	}
}

pub struct MockFallback;
impl pallet_session::SessionManager<u64> for MockFallback {
	fn new_session(_new_index: u32) -> Option<Vec<u64>> {
		None
	}
	fn start_session(_start_index: u32) {}
	fn end_session(_end_index: u32) {}
}

impl OnOffenceHandler<u64, (u64, sp_staking::Exposure<u64, u128>), Weight> for MockFallback {
	fn on_offence(
		_offenders: &[sp_staking::offence::OffenceDetails<
			u64,
			(u64, sp_staking::Exposure<u64, u128>),
		>],
		_slash_fraction: &[Perbill],
		_slash_session: u32,
	) -> Weight {
		Weight::zero()
	}
}

impl frame_support::traits::RewardsReporter<u64> for MockFallback {
	fn reward_by_ids(_rewards_by_ids: impl IntoIterator<Item = (u64, u32)>) {}
}

impl pallet_authorship::EventHandler<u64, u64> for MockFallback {
	fn note_author(_author: u64) {}
}

pub struct MockUnixTime;
impl frame_support::traits::UnixTime for MockUnixTime {
	fn now() -> core::time::Duration {
		core::time::Duration::from_secs(1234567890)
	}
}

parameter_types! {
	pub const MinimumValidatorSetSize: u32 = 3;
	pub const PointsPerBlock: u32 = 1;
	pub const MaxOffenceBatchSize: u32 = 100;
	pub static SetKeysCalls: Vec<(u64, MockSessionKeys)> = vec![];
	pub static SetKeysError: Option<DispatchError> = None;
	pub static PurgeKeysCalls: Vec<u64> = vec![];
	pub static PurgeKeysError: Option<DispatchError> = None;
	pub static RegisteredKeys: Vec<u64> = vec![];
	pub static KeysStateCalls: Vec<(u64, bool)> = vec![];
	pub static NextKeysStateSendFails: bool = false;
}

/// Records the key state reported to AssetHub, so tests can assert on it.
pub struct MockSendToAssetHub;
impl SendToAssetHub for MockSendToAssetHub {
	type AccountId = u64;

	fn relay_session_report(_report: rc_client::SessionReport<Self::AccountId>) -> Result<(), ()> {
		unimplemented!()
	}

	fn relay_new_offence_paged(
		_offences: Vec<(u32, rc_client::Offence<Self::AccountId>)>,
	) -> Result<(), ()> {
		unimplemented!()
	}

	fn relay_keys_state(stash: Self::AccountId, has_keys: bool) -> Result<(), ()> {
		// `::take` resets it to `false`, so a test arms one failure at a time.
		if NextKeysStateSendFails::take() {
			return Err(());
		}
		KeysStateCalls::mutate(|calls| calls.push((stash, has_keys)));
		Ok(())
	}
}

impl Config for Test {
	type CurrencyBalance = u128;
	type AssetHubOrigin = frame_system::EnsureRoot<u64>;
	type AdminOrigin = frame_system::EnsureRoot<u64>;
	type SendToAssetHub = MockSendToAssetHub;
	type MinimumValidatorSetSize = MinimumValidatorSetSize;
	type MaximumValidatorsWithPoints = ConstU32<128>;
	type UnixTime = MockUnixTime;
	type PointsPerBlock = PointsPerBlock;
	type MaxOffenceBatchSize = MaxOffenceBatchSize;
	type SessionInterface = MockSessionInterface;
	type Fallback = MockFallback;
	type MaxSessionReportRetries = ConstU32<3>;
}

#[cfg(test)]
pub fn new_test_ext() -> sp_io::TestExternalities {
	SetKeysCalls::take();
	SetKeysError::take();
	PurgeKeysCalls::take();
	PurgeKeysError::take();
	RegisteredKeys::take();
	KeysStateCalls::take();
	NextKeysStateSendFails::take();
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
