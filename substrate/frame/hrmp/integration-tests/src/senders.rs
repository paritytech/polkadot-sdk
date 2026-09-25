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
use hrmp_primitives::{MessageToPara, MessageToRelay, ParaNotification, ParaRequest};
use pallet_hrmp_para::SendToRelay;
use pallet_hrmp_relay::{ForwardToPara, NotifyParachain, SendToPara};
use polkadot_parachain_primitives::primitives::Id as PolkadotParaId;
use polkadot_runtime_parachains::{
	configuration, dmp as parachains_dmp, Origin as ParachainsOrigin,
};
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
	/// Index of `fn receive_request` in `pallet-hrmp-para`.
	#[codec(index = 1)]
	ReceiveRequest(hrmp_primitives::ParaId, ParaRequest),
}

/// The `UnpaidExecution + Transact` program the relay chain sends to the parachain.
///
/// `OriginKind::Superuser` so it lands as `Root` via `ParentAsSuperuser`.
fn transact_to_para(call: HrmpParaCalls) -> Result<(), ()> {
	let call = ParaRuntimePallets::Hrmp(call).encode();
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
pub struct RelaySendToPara;

impl SendToPara for RelaySendToPara {
	fn send(message: MessageToPara) -> Result<(), ()> {
		transact_to_para(HrmpParaCalls::Receive(message))
	}
}

/// Carries a request from any parachain to the one that owns channel management.
pub struct RelayForwardToPara;

impl ForwardToPara for RelayForwardToPara {
	fn forward(para_id: hrmp_primitives::ParaId, request: ParaRequest) -> Result<(), ()> {
		transact_to_para(HrmpParaCalls::ReceiveRequest(para_id, request))
	}
}

/// Delivers a channel notification to any para.
///
/// Queued straight onto DMP, the way `parachains_hrmp` sends the very same instructions, so it
/// reaches paras the simulator network does not model. Not a `Transact` for the three that are
/// XCM instructions: a para's XCM config already handles those.
pub struct RelayNotifyParachain;

impl NotifyParachain for RelayNotifyParachain {
	fn notify(para_id: hrmp_primitives::ParaId, notification: ParaNotification) -> Result<(), ()> {
		use xcm::opaque::{latest::Xcm as OpaqueXcm, VersionedXcm};

		let as_xcm = |instruction| VersionedXcm::from(OpaqueXcm(vec![instruction])).encode();
		let message = match notification {
			ParaNotification::NewChannelOpenRequest { sender, max_message_size, max_capacity } => {
				as_xcm(HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity })
			},
			ParaNotification::ChannelAccepted { recipient } => {
				as_xcm(HrmpChannelAccepted { recipient })
			},
			ParaNotification::ChannelClosing { initiator, sender, recipient } => {
				as_xcm(HrmpChannelClosing { initiator, sender, recipient })
			},
			// XCM has no instruction that concludes an open request, so these go down as the bare
			// notification. A production relay chain must pick a wire format: `Transact` into a
			// receiving pallet on the para, or a new instruction.
			conclusion => conclusion.encode(),
		};

		let config = configuration::ActiveConfig::<crate::relay::Runtime>::get();
		parachains_dmp::Pallet::<crate::relay::Runtime>::queue_downward_message(
			&config,
			para_id.into(),
			message,
		)
		.map_err(|_| ())
	}
}

frame_support::parameter_types! {
	pub const HrmpParaId: PolkadotParaId = PolkadotParaId::new(PARA_ID);
}

/// Accepts any parachain, resolved to its id.
///
/// What the relay chain vouches for when it forwards a request: the id comes from the origin the
/// XCM origin converter produced, never from the payload.
pub struct EnsureAnyParachain;

impl frame_support::traits::EnsureOrigin<crate::relay::RuntimeOrigin> for EnsureAnyParachain {
	type Success = hrmp_primitives::ParaId;

	fn try_origin(
		o: crate::relay::RuntimeOrigin,
	) -> Result<Self::Success, crate::relay::RuntimeOrigin> {
		let parachain_origin: Result<ParachainsOrigin, _> = o.clone().into();
		match parachain_origin {
			Ok(ParachainsOrigin::Parachain(id)) => Ok(id.into()),
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<crate::relay::RuntimeOrigin, ()> {
		Ok(ParachainsOrigin::Parachain(HrmpParaId::get()).into())
	}
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
