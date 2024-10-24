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

use std::collections::BTreeMap;

type Block = frame_system::mocking::MockBlock<Test>;

type AccountId = u64;
type AuthorshipPoints = u32;
type Weight = sp_runtime::Weight;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		RCClient: pallet_rc_client,
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

parameter_types! {
	pub static MaxOffenders: u32 = 5;
	pub static MaxValidators: u32 = 10;
}

impl crate::pallet::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Staking = MockStaking;
	type MaxOffenders = MaxOffenders;
	type MaxValidators = MaxValidators;
	type SessionKeys = MockSessionKeys;
	type SessionKeysHandler = impls::SessionKeysHandlerXCM<Self>;
	type ValidatorSetHandler = impls::ValidatorSetHandlerXCM<Self>;
}

parameter_types! {
	pub static AuthoringState: Vec<(AccountId, SessionIndex, AuthorshipPoints)> = vec![];
	pub static OffencesState: BTreeMap<SessionIndex, (AccountId, AccountId, Perbill)> = BTreeMap::new();
}

pub struct MockStaking;

impl<BlockNumber> AuthorshipEventHandler<AccountId, BlockNumber> for MockStaking {
	fn note_author(author: AccountId) {
		// one point per authoring.
		AuthoringState::mutate(|s| {
			let session = 0; // TODO
			s.push((author, session, 1))
		});
	}
}

impl OnOffenceHandler<AccountId, AccountId, Weight> for MockStaking {
	fn on_offence(
		_offenders: &[sp_staking::offence::OffenceDetails<AccountId, AccountId>],
		_slash_fraction: &[Perbill],
		_session: SessionIndex,
	) -> Weight {
		todo!()
	}
}
