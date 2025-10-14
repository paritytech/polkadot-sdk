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

type Block = frame_system::mocking::MockBlock<Runtime>;

const TARGET_BLOCK_RATE: u32 = 12;

#[docify::export(tx_extension_setup)]
pub type TxExtension = DynamicMaxBlockWeight<
	Runtime,
	// Here you need to set the other extensions that are required by your runtime...
	(
		frame_system::AuthorizeCall<Runtime>,
		frame_system::CheckNonZeroSender<Runtime>,
		frame_system::CheckSpecVersion<Runtime>,
		frame_system::CheckGenesis<Runtime>,
		frame_system::CheckEra<Runtime>,
		frame_system::CheckNonce<Runtime>,
		frame_system::CheckWeight<Runtime>,
	),
	ConstU32<TARGET_BLOCK_RATE>,
>;

#[docify::export_content(max_block_weight_setup)]
mod max_block_weight_setup {
	type MaximumBlockWeight = MaxParachainBlockWeight<Runtime, ConstU32<TARGET_BLOCK_RATE>>;

	parameter_types! {
		pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
			.base_block(BlockExecutionWeight::get())
			.for_class(DispatchClass::all(), |weights| {
				weights.base_extrinsic = ExtrinsicBaseWeight::get();
			})
			.for_class(DispatchClass::Normal, |weights| {
				weights.max_total = Some(NORMAL_DISPATCH_RATIO * MaximumBlockWeight::get());
			})
			.for_class(DispatchClass::Operational, |weights| {
				weights.max_total = Some(MaximumBlockWeight::get());
				// Operational transactions have some extra reserved space, so that they
				// are included even if block reached `MaximumBlockWeight`.
				weights.reserved = Some(
					MaximumBlockWeight::get() - NORMAL_DISPATCH_RATIO * MaximumBlockWeight::get()
				);
			})
			.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
			.build_or_panic();
	}
}

// Configure a mock runtime to test the functionality
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
#[docify::export(pre_inherents_setup)]
impl frame_system::Config for Runtime {
	// Setup the block weight.
	type BlockWeights = RuntimeBlockWeights;
	// Set the `PreInherents` hook.
	type PreInherents = DynamicMaxBlockWeightHooks<Runtime, ConstU32<TARGET_BLOCK_RATE>>;

	// Rest of the types is omitted here.
	type Block = Block;
	type OnSetCode = crate::ParachainSetCode<Runtime>;
}

impl crate::Config for Runtime {
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
	pub enum Runtime {
		System: frame_system,
		ParachainSystem: parachain_system,
	}
);

pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
>;

pub fn new_test_ext_with_digest(num_cores: Option<u16>) -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

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
