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
	dispatch::DispatchClass,
	parameter_types,
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight},
		Weight,
	},
};
use frame_system::limits::BlockWeights;
use sp_core::ConstU32;
use sp_io;
use sp_runtime::{BuildStorage, Perbill};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(10);

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
	use super::*;

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
	type BlockWeights = max_block_weight_setup::RuntimeBlockWeights;
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

			frame_system::Pallet::<Runtime>::deposit_log(digest);
		}
	});

	ext
}

/// Helper to create test externalities with core and bundle info
pub fn new_test_ext_with_bundle(
	num_cores: Option<u16>,
	bundle_index: u8,
	maybe_last: bool,
) -> sp_io::TestExternalities {
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
			frame_system::Pallet::<Runtime>::deposit_log(digest);
		}

		let bundle_info = BundleInfo { index: bundle_index, maybe_last };
		let digest = CumulusDigestItem::BundleInfo(bundle_info).to_digest_item();
		frame_system::Pallet::<Runtime>::deposit_log(digest);
	});

	ext
}

/// Helper to create test externalities for first block in core
pub fn new_test_ext_first_block(num_cores: u16) -> sp_io::TestExternalities {
	new_test_ext_with_bundle(Some(num_cores), 0, false)
}

/// Helper to create test externalities for non-first block in core
pub fn new_test_ext_non_first_block(num_cores: u16) -> sp_io::TestExternalities {
	new_test_ext_with_bundle(Some(num_cores), 1, false)
}

/// Helper to check if UseFullCore digest was deposited
pub fn has_use_full_core_digest() -> bool {
	use codec::Decode;
	use cumulus_primitives_core::CUMULUS_CONSENSUS_ID;
	use sp_runtime::DigestItem;

	let digest = frame_system::Pallet::<Runtime>::digest();
	digest.logs.iter().any(|log| match log {
		DigestItem::Consensus(id, val) if id == &CUMULUS_CONSENSUS_ID => {
			if let Ok(CumulusDigestItem::UseFullCore) = CumulusDigestItem::decode(&mut &val[..]) {
				true
			} else {
				false
			}
		},
		_ => false,
	})
}

/// Helper to register weight as consumed (simulating on_initialize)
pub fn register_weight(weight: Weight) {
	frame_system::Pallet::<Runtime>::register_extra_weight_unchecked(
		weight,
		DispatchClass::Mandatory,
	);
}
