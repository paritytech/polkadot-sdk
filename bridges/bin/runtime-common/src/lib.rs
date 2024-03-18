// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Common types/functions that may be used by runtimes of all bridged chains.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use bp_polkadot_core::parachains::{ParaHash, ParaId};
use bp_runtime::{Parachain, ParachainIdOf};
use pallet_bridge_grandpa::Config as GrandpaConfig;
use pallet_bridge_parachains::{
	Config as ParachainsConfig, ParachainHeadsUpdateFilter, RelayBlockHash, RelayBlockNumber,
};
use sp_runtime::traits::{Get, PhantomData};

pub mod extensions;
pub mod messages;
pub mod messages_api;
pub mod messages_benchmarking;
pub mod messages_call_ext;
pub mod messages_generation;
pub mod messages_xcm_extension;
pub mod parachains_benchmarking;

mod mock;

#[cfg(feature = "integrity-test")]
pub mod integrity;

const LOG_TARGET_BRIDGE_DISPATCH: &str = "runtime::bridge-dispatch";

/// Trait identifying a bridged parachain. A relayer might be refunded for delivering messages
/// coming from this parachain.
pub trait RefundableParachainId {
	/// The instance of the bridge parachains pallet.
	type Instance: 'static;
	/// The parachain Id.
	type Id: Get<u32>;
}

/// Default implementation of `RefundableParachainId`.
pub struct DefaultRefundableParachainId<Instance, Id>(PhantomData<(Instance, Id)>);

impl<Instance, Id> RefundableParachainId for DefaultRefundableParachainId<Instance, Id>
where
	Instance: 'static,
	Id: Get<u32>,
{
	type Instance = Instance;
	type Id = Id;
}

/// Implementation of `RefundableParachainId` for `trait Parachain`.
pub struct RefundableParachain<Instance, Para>(PhantomData<(Instance, Para)>);

impl<Instance, Para> RefundableParachainId for RefundableParachain<Instance, Para>
where
	Instance: 'static,
	Para: Parachain,
{
	type Instance = Instance;
	type Id = ParachainIdOf<Para>;
}

/// A filter that allows one free parachain head submissions for every free
/// relay chain header. It DOES NOT refund for parachains, finalized at
/// mandatory relay chain blocks.
///
/// The number of free submissions is implicitly limited by the
/// `T::MaxFreeHeadersPerBlock` - there can be at most one parachain head
/// submission for every free relay chain header. And since number of free
/// relay chain headers is limited by this parameter, free parachain head
/// updates is also limited.
pub struct FreeParachainUpdateForFreeRelayHeader<T, GI, P>(PhantomData<(T, GI, P)>);

impl<T, GI, P> ParachainHeadsUpdateFilter for FreeParachainUpdateForFreeRelayHeader<T, GI, P>
where
	T: GrandpaConfig<GI> + ParachainsConfig<P::Instance>,
	GI: 'static,
	P: RefundableParachainId,
{
	fn is_free(
		at_relay_block: (RelayBlockNumber, RelayBlockHash),
		parachains: &[(ParaId, ParaHash)],
	) -> bool {
		// just one parachain that we are interested in
		if parachains.len() != 1 || parachains[0].0 .0 != P::Id::get() {
			return false;
		}

		// we only refund for parachains, finalized at free relay chain blocks
		let Some(free_headers_interval) = <T as GrandpaConfig<GI>>::FreeHeadersInterval::get()
		else {
			return false
		};
		if at_relay_block.0 != 0 && at_relay_block.0 % free_headers_interval == 0 {
			return true
		}

		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use mock::*;

	type RefundableParachain = super::RefundableParachain<(), BridgedUnderlyingParachain>;
	type FreeBridgedParachainUpdate =
		FreeParachainUpdateForFreeRelayHeader<TestRuntime, (), RefundableParachain>;

	#[test]
	fn free_parachain_update_for_free_relay_header_works() {
		let free_interval = <TestRuntime as GrandpaConfig<()>>::FreeHeadersInterval::get();
		// not free when there are multiple parachains
		assert!(!FreeBridgedParachainUpdate::is_free(
			(free_interval, Default::default()),
			&[
				(ParaId(BridgedUnderlyingParachain::PARACHAIN_ID), Default::default()),
				(ParaId(BridgedUnderlyingParachain::PARACHAIN_ID + 1), Default::default()),
			],
		));
		// not free when finalized at non-free relay chain header
		assert!(!FreeBridgedParachainUpdate::is_free(
			(free_interval + 1, Default::default()),
			&[(ParaId(BridgedUnderlyingParachain::PARACHAIN_ID), Default::default()),],
		));
		// not free when finalized at relay chain genesis
		assert!(!FreeBridgedParachainUpdate::is_free(
			(0, Default::default()),
			&[(ParaId(BridgedUnderlyingParachain::PARACHAIN_ID), Default::default()),],
		));
		// free when finalized at free relay chain header
		assert!(FreeBridgedParachainUpdate::is_free(
			(free_interval, Default::default()),
			&[(ParaId(BridgedUnderlyingParachain::PARACHAIN_ID), Default::default()),],
		));
	}
}
