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

//! The XCM transport between the two HRMP pallets.
//!
//! Neither pallet knows anything about XCM; this is the runtime-level glue that a real Coretime
//! chain and relay chain would each supply. The remote call is hand-encoded through an enum whose
//! `#[codec(index)]`s mirror the other chain's pallet index and call index, exactly as
//! `asset-hub-westend/src/staking.rs` and `westend/src/lib.rs` do for staking-async.

use codec::Encode;
use frame_support::traits::{CallerTrait, OriginTrait};
use hrmp_primitives::{MessageToPara, MessageToRelay};
use pallet_hrmp_para::SendToRelay;
use pallet_hrmp_relay::SendToPara;
use polkadot_parachain_primitives::primitives::Id as PolkadotParaId;
use polkadot_runtime_parachains::Origin as ParachainsOrigin;
use xcm::latest::prelude::*;

/// The para id of the control-plane parachain in this test network.
pub const PARA_ID: u32 = 1000;

/// Calls on the relay chain, as the parachain must encode them.
///
/// Audit: index of `Hrmp` (`pallet-hrmp-relay`) in the relay chain's `construct_runtime!`, in
/// `crate::relay`.
#[derive(Encode)]
pub enum RelayRuntimePallets {
	#[codec(index = 8)]
	Hrmp(HrmpRelayCalls),
}

#[derive(Encode)]
pub enum HrmpRelayCalls {
	/// Index of `fn receive` in `pallet-hrmp-relay`.
	#[codec(index = 0)]
	Receive(MessageToRelay),
}

/// Calls on the parachain, as the relay chain must encode them.
///
/// Audit: index of `Hrmp` (`pallet-hrmp-para`) in the parachain's `construct_runtime!`, in
/// `crate::para`.
#[derive(Encode)]
pub enum ParaRuntimePallets {
	#[codec(index = 4)]
	Hrmp(HrmpParaCalls),
}

#[derive(Encode)]
pub enum HrmpParaCalls {
	/// Index of `fn receive` in `pallet-hrmp-para`.
	#[codec(index = 0)]
	Receive(MessageToPara),
}

/// The parachain's half of the transport.
///
/// `OriginKind::Native` so the message lands on the relay chain as
/// `origin::Origin::Parachain(PARA_ID)`, which is what [`EnsureHrmpPara`] accepts.
pub struct ParaSendToRelay;

impl SendToRelay for ParaSendToRelay {
	fn send(message: MessageToRelay) -> Result<(), ()> {
		let call = RelayRuntimePallets::Hrmp(HrmpRelayCalls::Receive(message)).encode();
		let program = Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Native,
				fallback_max_weight: None,
				call: call.into(),
			},
		]);

		send_xcm::<crate::para::XcmRouter>(Location::parent(), program)
			.map(|_| ())
			.map_err(|_| ())
	}
}

/// The relay chain's half of the transport.
///
/// `OriginKind::Superuser` so the report lands on the parachain as `Root` via
/// `ParentAsSuperuser`.
pub struct RelaySendToPara;

impl SendToPara for RelaySendToPara {
	fn send(message: MessageToPara) -> Result<(), ()> {
		let call = ParaRuntimePallets::Hrmp(HrmpParaCalls::Receive(message)).encode();
		let program = Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Superuser,
				fallback_max_weight: None,
				call: call.into(),
			},
		]);

		let dest = Location::new(0, [Junction::Parachain(PARA_ID)]);
		send_xcm::<crate::relay::XcmRouter>(dest, program).map(|_| ()).map_err(|_| ())
	}
}

frame_support::parameter_types! {
	pub const HrmpParaId: PolkadotParaId = PolkadotParaId::new(PARA_ID);
}

/// Accepts Root, or the one parachain that is allowed to drive channel management.
///
/// The same shape as westend's `EnsureAssetHub`: match on the parachain origin the XCM origin
/// converter produced, and check the id.
pub struct EnsureHrmpPara;

impl frame_support::traits::EnsureOrigin<crate::relay::RuntimeOrigin> for EnsureHrmpPara {
	type Success = ();

	fn try_origin(
		o: crate::relay::RuntimeOrigin,
	) -> Result<Self::Success, crate::relay::RuntimeOrigin> {
		if o.caller().is_root() {
			return Ok(());
		}

		let parachain_origin: Result<ParachainsOrigin, _> = o.clone().into();
		match parachain_origin {
			Ok(ParachainsOrigin::Parachain(id)) if id == HrmpParaId::get() => Ok(()),
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<crate::relay::RuntimeOrigin, ()> {
		Ok(crate::relay::RuntimeOrigin::root())
	}
}
