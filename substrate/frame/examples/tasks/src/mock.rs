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

//! Mock runtime for `tasks-example` tests.
#![cfg(test)]

use crate::{self as pallet_example_tasks};
use frame_support::derive_impl;
use sp_runtime::testing::UintAuthorityId;

pub type AccountId = u32;
pub type Balance = u32;

pub type TransactionExtension = (frame_system::AuthorizeCall<Runtime>,);
pub type Header = sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, Extrinsic>;
pub type Extrinsic = sp_runtime::generic::UncheckedExtrinsic<
	u64,
	RuntimeCall,
	UintAuthorityId,
	TransactionExtension,
>;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		TasksExample: pallet_example_tasks,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

impl<LocalCall> frame_system::offchain::CreateTransactionBase<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type RuntimeCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type Extension = TransactionExtension;

	fn create_transaction(call: RuntimeCall, extension: Self::Extension) -> Self::Extrinsic {
		Extrinsic::new_transaction(call, extension)
	}
}

impl<LocalCall> frame_system::offchain::CreateAuthorizedTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_extension() -> Self::Extension {
		(frame_system::AuthorizeCall::<Runtime>::new(),)
	}
}

impl<LocalCall> frame_system::offchain::CreateBare<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
		Extrinsic::new_bare(call)
	}
}

impl pallet_example_tasks::Config for Runtime {
	type RuntimeTask = RuntimeTask;
	type WeightInfo = ();
}

pub fn advance_to(b: u64) {
	#[cfg(feature = "experimental")]
	use frame_support::traits::Hooks;
	while System::block_number() < b {
		System::set_block_number(System::block_number() + 1);
		#[cfg(feature = "experimental")]
		TasksExample::offchain_worker(System::block_number());
	}
}
