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

use crate::{
	AccountId, AuraId, Runtime, System, RELAY_CHAIN_SLOT_DURATION_MILLIS, RELAY_PARENT_OFFSET,
};
use alloc::vec::Vec;
use core::time::Duration;
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use frame_support::traits::{ConstU32, UnixTime};
use frame_system::EnsureRoot;
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

/// The latest time the relay chain can be at, given the relay parent of the current block.
///
/// The relay parent's slot, moved forward by the relay chain's `allowed_ancestry_len`, in relay
/// chain slots of [`RELAY_CHAIN_SLOT_DURATION_MILLIS`]. The relay chain accepts relay parents at
/// most `allowed_ancestry_len` blocks below its newest block, so this is the start of the newest
/// relay slot the relay chain can be at, when no relay slot was skipped.
///
/// It is up to about one relay slot ahead of real time, when the author built on a newer relay
/// parent than the oldest allowed, as it does by default with a `RELAY_PARENT_OFFSET` smaller than
/// `allowed_ancestry_len`. It lags real time by up to one relay slot when the author built on the
/// oldest allowed relay parent, plus one per relay slot skipped below the newest relay block.
/// Assuming the oldest relay parent keeps the lag, and so how long old reports count, as small as
/// the block allows.
///
/// `allowed_ancestry_len` comes from the relay chain configuration in the relay chain state proof;
/// [`RELAY_PARENT_OFFSET`] is used until one is processed. `0` until the first relay slot is known.
pub struct LatestRelayTime;

impl UnixTime for LatestRelayTime {
	fn now() -> Duration {
		let slot = cumulus_pallet_aura_ext::Pallet::<Runtime>::relay_slot().map_or(0, |s| *s);
		let ancestry = cumulus_pallet_parachain_system::HostConfiguration::<Runtime>::get()
			.map_or(RELAY_PARENT_OFFSET, |config| config.async_backing_params.allowed_ancestry_len);
		let slots = slot.saturating_add(ancestry.into());
		Duration::from_millis(slots.saturating_mul(RELAY_CHAIN_SLOT_DURATION_MILLIS.into()))
	}
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
	type AdminOrigin = EnsureRoot<AccountId>;
	type OnPriceUpdate = ();
	type WeightInfo = ();
}
