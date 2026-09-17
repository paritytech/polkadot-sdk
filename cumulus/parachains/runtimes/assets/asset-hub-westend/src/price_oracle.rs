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
//! Configuration of the price oracle: the pairs, the accepted signers and the pallet.

use crate::{AccountId, AuraId, Runtime, System};
use alloc::vec::Vec;
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use frame_support::traits::ConstU32;
use frame_system::EnsureRoot;
use pallet_price_oracle::{Pairs, Signers};
use sp_price_oracle::PairId;

/// The pairs priced on Asset Hub. Discriminants are the wire identifiers and must never change.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum Pair {
	DotUsdt = 1,
	DotUsd = 2,
	UsdtUsd = 3,
}

impl TryFrom<PairId> for Pair {
	type Error = ();

	fn try_from(id: PairId) -> Result<Self, ()> {
		Ok(match id.0 {
			1 => Pair::DotUsdt,
			2 => Pair::DotUsd,
			3 => Pair::UsdtUsd,
			_ => return Err(()),
		})
	}
}

impl From<Pair> for PairId {
	fn from(pair: Pair) -> PairId {
		PairId(pair as u16)
	}
}

impl Pairs for Pair {
	fn all() -> Vec<PairId> {
		alloc::vec![Pair::DotUsdt.into(), Pair::DotUsd.into(), Pair::UsdtUsd.into()]
	}

	fn conversions(pair: PairId) -> Vec<(PairId, PairId)> {
		match Pair::try_from(pair) {
			Ok(Pair::DotUsd) => alloc::vec![(Pair::DotUsdt.into(), Pair::UsdtUsd.into())],
			_ => Vec::new(),
		}
	}
}

/// The collators: the current Aura authorities and the ones queued for the next session.
pub struct Collators;

impl Signers<AuraId> for Collators {
	fn signers() -> Vec<AuraId> {
		let mut signers: Vec<AuraId> = pallet_aura::Authorities::<Runtime>::get().into_inner();
		for (_, keys) in pallet_session::QueuedKeys::<Runtime>::get() {
			if !signers.contains(&keys.aura) {
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
	type MaxSigners = ConstU32<1_000>;
	type Pairs = Pair;
	type AnchorProvider = System;
	type BlockNumberProvider = RelaychainDataProvider<Runtime>;
	type AdminOrigin = EnsureRoot<AccountId>;
	type OnPriceUpdate = ();
	type WeightInfo = ();
}
