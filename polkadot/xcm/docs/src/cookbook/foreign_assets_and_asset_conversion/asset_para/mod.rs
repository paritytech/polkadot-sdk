// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! # Runtime

use cumulus_pallet_parachain_system::consensus_hook::RequireParentIncluded;
use cumulus_pallet_parachain_system::RelayNumberStrictlyIncreases;
use frame::{
	deps::{frame_support, frame_system, sp_core},
	runtime::prelude::*,
	traits::IdentityLookup,
};
use parachains_common::message_queue::NarrowOriginToSibling;
use xcm_executor::XcmExecutor;
use xcm_simulator::mock_message_queue;
use cumulus_primitives_core::AggregateMessageOrigin;

mod xcm_config;

mod assets;

use xcm_config::XcmConfig;

pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type AccountId = frame::deps::sp_runtime::AccountId32;
pub type Balance = u128;

construct_runtime! {
	pub struct Runtime {
		System: frame_system,
		ParachainSystem: cumulus_pallet_parachain_system,
		ParachainInfo: parachain_info,
		ForeignAssets: pallet_assets::<Instance1>,
		PoolAssets: pallet_assets::<Instance2>,
		AssetConversion: pallet_asset_conversion,
		Balances: pallet_balances,

		// Xcm Helpers
		XcmpQueue: cumulus_pallet_xcmp_queue,
		XcmPallet: pallet_xcm,
		CumulusXcm: cumulus_pallet_xcm,
		MessageQueue: pallet_message_queue
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
}

pub const UNITS: Balance = 10_000_000_000;

parameter_types! {
	pub const ExistentialDeposit: Balance = UNITS;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
}

impl parachain_info::Config for Runtime {}

parameter_types! {
	pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type DmpQueue =
		frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
	type ReservedDmpWeight = ();
	type OutboundXcmpMessageSource = XcmpQueue;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ();
	type CheckAssociatedRelayNumber = RelayNumberStrictlyIncreases;
	type ConsensusHook = RequireParentIncluded;
	type SelectCore = cumulus_pallet_parachain_system::DefaultCoreSelector<Runtime>;
	type RelayParentOffset = ();
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MessageProcessor = xcm_builder::ProcessXcmMessage<
		AggregateMessageOrigin,
		xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
		RuntimeCall,
	>;
	type Size = u32;
	// The XCMP queue pallet is only ever able to handle the `Sibling(ParaId)` origin:
	type QueueChangeHandler = NarrowOriginToSibling<XcmpQueue>;
	type QueuePausedQuery = NarrowOriginToSibling<XcmpQueue>;
	type HeapSize = sp_core::ConstU32<{ 64 * 1024 }>;
	type MaxStale = sp_core::ConstU32<8>;
	type ServiceWeight = ();
	type IdleMaxServiceWeight = ();
}
