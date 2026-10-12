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
//! Configuration of the price oracle: the accepted signers and the pallet.

use crate::{AccountId, AuraId, Runtime, System};
use alloc::vec::Vec;
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use frame_support::traits::ConstU32;
use frame_system::EnsureRoot;
use pallet_price_oracle::Signers;

/// The collators: the current Aura authorities and the ones queued for the next session.
pub struct Collators;

impl Signers<AuraId> for Collators {
	fn signers() -> Vec<AuraId> {
		let mut signers: Vec<AuraId> = pallet_aura::Authorities::<Runtime>::get().into_inner();
		for (_, keys) in pallet_session::QueuedKeys::<Runtime>::get() {
			if !signers.contains(&keys.aura) {
				// TODO: should happen only at the end of the current session.
				signers.push(keys.aura);
			}
		}
		signers
	}
}

impl pallet_price_oracle::Config for Runtime {
	type SignerId = AuraId;
	type SignerSignature = sp_consensus_aura::sr25519::AuthoritySignature;
	type Signers = Collators;
	type MaxSigners = ConstU32<600>;
	type MaxCrossRates = ConstU32<4>;
	type AnchorProvider = System;
	type BlockNumberProvider = RelaychainDataProvider<Runtime>;
	type AdminOrigin = EnsureRoot<AccountId>;
	type OnPriceUpdate = ();
	type WeightInfo = ();
}
