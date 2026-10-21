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

//! Test runtime for `pallet-hrmp-relay`.

use crate as pallet_hrmp_relay;
use frame_support::{derive_impl, parameter_types, traits::ConstU32, weights::Weight};
use polkadot_primitives::Id as ParaId;
use polkadot_runtime_parachains::{
	hrmp::deposits::{DepositAction, DepositChangeRequest, DepositError, OnDepositSet},
	origin as parachains_origin,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		ParachainsOrigin: parachains_origin,
		HrmpRelay: pallet_hrmp_relay,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl parachains_origin::Config for Test {}

/// Ignores every answer.
pub struct NoOnDepositSet;
impl OnDepositSet for NoOnDepositSet {
	fn on_deposit_set(_: DepositChangeRequest, _: Result<(), DepositError>) {}

	fn weight_of_on_deposit_set(_: &DepositAction) -> Weight {
		Weight::zero()
	}
}

parameter_types! {
	pub const HolderParaId: ParaId = ParaId::new(1005);
}

impl pallet_hrmp_relay::Config for Test {
	type OnDepositSet = NoOnDepositSet;
	type HolderParaId = HolderParaId;
	type SendToPara = ();
	type MaxOutbox = ConstU32<4>;
	type WeightInfo = ();
}
