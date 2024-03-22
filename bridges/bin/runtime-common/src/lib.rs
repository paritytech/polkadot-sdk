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

use bp_runtime::{Parachain, ParachainIdOf};
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
