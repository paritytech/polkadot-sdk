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

use crate::{AccountId, AuraId, Runtime, System, RELAY_CHAIN_SLOT_DURATION_MILLIS};
use alloc::{vec, vec::Vec};
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use frame_support::{
	ord_parameter_types,
	traits::{ConstU32, EitherOfDiverse},
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_price_oracle::Signers;

/// The collators: the current Aura authorities and the ones queued for the next session.
pub struct Collators;

impl Signers<AuraId> for Collators {
	fn signers() -> Vec<AuraId> {
		let mut signers: Vec<AuraId> = pallet_aura::Authorities::<Runtime>::get().into_inner();
		for (_, keys) in pallet_session::QueuedKeys::<Runtime>::get() {
			if !signers.contains(&keys.aura) {
				// TODO: accept the queued set only a few blocks before a session change, for nodes
				// lagging behind it, and the previous set for one report window after it, for the
				// outgoing collators' last reports.
				signers.push(keys.aura);
			}
		}
		signers
	}
}

/// The latest time the relay chain can be at, given the relay parent of the current block. See
/// [`parachains_common::impls::LatestRelayTime`].
pub type LatestRelayTime =
	parachains_common::impls::LatestRelayTime<Runtime, RELAY_CHAIN_SLOT_DURATION_MILLIS>;

ord_parameter_types! {
	/// A pure proxy that may configure the oracle besides Root, so it can be set up on Westend
	/// without governance. `5F66jpJ9M8mJXhDZDc3g3zVFoidvWudX3HYJgVp55EBpNnuQ`.
	pub const OracleAdmin: AccountId = AccountId::new(hex_literal::hex!(
		"85c4f913296ea55a4762391d744b7fe20c5a968cc13e700bad7d69362a87c5d6"
	));
}

impl pallet_price_oracle::Config for Runtime {
	type SignerId = AuraId;
	type SignerSignature = sp_consensus_aura::sr25519::AuthoritySignature;
	type Signers = Collators;
	type MaxSigners = ConstU32<600>;
	type MarketPricing = pallet_price_oracle::order_book::OrderBookPricing;
	type MaxCrossRates = ConstU32<4>;
	type AnchorProvider = (System, LatestRelayTime);
	type BlockNumberProvider = RelaychainDataProvider<Runtime>;
	type AdminOrigin =
		EitherOfDiverse<EnsureRoot<AccountId>, EnsureSignedBy<OracleAdmin, AccountId>>;
	type OnPriceUpdate = ();
	type WeightInfo = ();
}
