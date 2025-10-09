// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use super::{transaction_extension::DynamicMaxBlockWeight, *};
use crate as parachain_system;
use codec::Compact;
use cumulus_primitives_core::{
	BundleInfo, ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem,
};
use frame_support::{
	construct_runtime, derive_impl,
	dispatch::{DispatchClass, DispatchInfo, Pays},
	traits::Hooks,
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight},
};
use frame_system::mocking::MockBlock;
use polkadot_primitives::MAX_POV_SIZE;
use sp_core::ConstU32;
use sp_io;
use sp_runtime::{
	generic::Header,
	testing::{TestXt, UintAuthorityId},
	traits::{
		BlakeTwo256, Block as BlockT, Dispatchable, Header as HeaderT, IdentityLookup,
		TransactionExtension,
	},
	transaction_validity::TransactionSource,
	BuildStorage, Perbill,
};

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the functionality
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = u64;
	type AccountData = ();
	type Lookup = IdentityLookup<Self::AccountId>;
	type OnSetCode = crate::ParachainSetCode<Test>;
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = ();
	type OutboundXcmpMessageSource = ();
	type DmpQueue = ();
	type ReservedDmpWeight = ();
	type XcmpMessageHandler = ();
	type ReservedXcmpWeight = ();
	type CheckAssociatedRelayNumber = crate::RelayNumberStrictlyIncreases;
	type WeightInfo = ();
	type ConsensusHook = crate::ExpectParentIncluded;
	type RelayParentOffset = ();
}

construct_runtime!(
	pub enum Test {
		System: frame_system,
		ParachainSystem: parachain_system,
	}
);

pub type Executive = frame_executive::Executive<
	Test,
	Block,
	frame_system::ChainContext<Test>,
	Test,
	AllPalletsWithSystem,
>;

pub fn new_test_ext_with_digest(num_cores: Option<u16>) -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let mut ext = sp_io::TestExternalities::from(storage);

	ext.execute_with(|| {
		if let Some(num_cores) = num_cores {
			let core_info = CoreInfo {
				selector: CoreSelector(0),
				claim_queue_offset: ClaimQueueOffset(0),
				number_of_cores: Compact(num_cores),
			};

			let digest = CumulusDigestItem::CoreInfo(core_info).to_digest_item();

			frame_system::Pallet::<Test>::deposit_log(digest);
		}
	});

	ext
}
