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

//! A para acting as itself on the registrar parachain, through the relay chain or directly.

use super::*;
use codec::Encode;
use registrar_primitives::{ParaRequest, ParaRequestV1};
use senders::{ParaRuntimePallets, RegistrarParaCalls, RegistrarRelayCalls, RelayRuntimePallets};
use xcm::latest::prelude::*;

/// `call`, run unpaid under the sender's own origin.
fn native_transact(call: Vec<u8>) -> Xcm<()> {
	Xcm(vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact { origin_kind: OriginKind::Native, fallback_max_weight: None, call: call.into() },
	])
}

/// Have [`ManagedPara`] send `request` over UMP, as a para with no channel to the registrar would.
fn relay_from_para(request: ParaRequestV1) {
	let call = RelayRuntimePallets::<sp_runtime::AccountId32>::Registrar(
		RegistrarRelayCalls::Relay(ParaRequest::V1(request)),
	)
	.encode();
	ManagedPara::execute_with(|| {
		assert_ok!(send_xcm::<para::XcmRouter>(Location::parent(), native_transact(call)));
	});
}

/// Whether the relay chain's registrar emitted an event matching `f`.
fn relay_event(f: impl Fn(&pallet_registrar_relay::Event<relay::Runtime>) -> bool) -> bool {
	relay::System::events().iter().any(|e| match &e.event {
		relay::RuntimeEvent::RegistrarRelay(event) => f(event),
		_ => false,
	})
}

/// Onboard the para [`ManagedPara`] runs as, managed by Alice.
fn managed_para() -> u32 {
	let para_id = onboarded_para(ALICE);
	assert_eq!(para_id, MANAGED_PARA_ID);
	para_id
}

fn relayed_ok(para_id: u32) -> pallet_registrar_para::Event<para::Runtime> {
	pallet_registrar_para::Event::RelayedRequest { para_id, result: Ok(()) }
}

#[test]
fn a_locked_para_unlocks_itself_through_the_relay_chain() {
	MockNet::reset();

	let para_id = managed_para();
	note_head_on_relay(para_id);
	RegistrarPara::execute_with(|| assert_eq!(para_lock(para_id), Some(true)));

	relay_from_para(ParaRequestV1::RemoveLock);

	Relay::execute_with(|| {
		assert!(relay_event(|e| *e == pallet_registrar_relay::Event::Relayed { para_id }));
	});
	RegistrarPara::execute_with(|| {
		assert_eq!(para_lock(para_id), Some(false));
		assert!(para_event(|e| *e == relayed_ok(para_id)));
	});
}

#[test]
fn a_relayed_lock_shuts_the_manager_out() {
	MockNet::reset();

	let para_id = managed_para();
	relay_from_para(ParaRequestV1::AddLock);

	RegistrarPara::execute_with(|| {
		assert_eq!(para_lock(para_id), Some(true));
		assert!(para_event(|e| *e == relayed_ok(para_id)));
		assert_noop!(
			para::Registrar::set_current_head(
				para::RuntimeOrigin::signed(ALICE),
				para_id,
				head(64)
			),
			pallet_registrar_para::Error::<para::Runtime>::ParaLocked,
		);
	});
}

#[test]
fn a_relayed_head_lands_on_the_relay_chain_while_the_manager_is_locked_out() {
	MockNet::reset();

	let para_id = managed_para();
	note_head_on_relay(para_id);

	relay_from_para(ParaRequestV1::SetCurrentHead { head: head(64) });

	Relay::execute_with(|| assert_eq!(relay_head(para_id), Some(HeadData(head(64)))));
	// Registration was message 0, so the head update is message 1.
	RegistrarPara::execute_with(|| {
		assert!(para_event(
			|e| *e == pallet_registrar_para::Event::HeadUpdated { para_id, message_id: 1 }
		));
		assert!(para_event(|e| *e == relayed_ok(para_id)));
	});
}

#[test]
fn a_relayed_code_upgrade_is_authorized_and_the_upload_schedules_it() {
	MockNet::reset();

	let para_id = managed_para();
	let blob = upgrade_code(96);

	relay_from_para(ParaRequestV1::ScheduleCodeUpgrade { code_hash: hash_of(&blob), code_len: 96 });

	Relay::execute_with(|| {
		assert!(
			pallet_registrar_relay::PendingCodeUpgrades::<relay::Runtime>::get(para_id).is_some()
		);
		assert_ok!(submit_upgrade(para_id, blob.clone()));
		assert_eq!(
			polkadot_runtime_parachains::paras::FutureCodeHash::<relay::Runtime>::get(
				polkadot_primitives::Id::from(para_id)
			),
			Some(ValidationCode(blob).hash())
		);
	});
	RegistrarPara::execute_with(|| {
		assert!(para_event(|e| matches!(
			e,
			pallet_registrar_para::Event::CodeUpgradeAuthorized { message_id: 1, .. }
		)));
		assert!(para_event(
			|e| *e == pallet_registrar_para::Event::CodeUpgradeScheduled { para_id, message_id: 1 }
		));
	});
}

#[test]
fn a_locked_para_deregisters_itself_through_the_relay_chain() {
	MockNet::reset();

	let para_id = managed_para();
	note_head_on_relay(para_id);
	let deposit = para::PARA_DEPOSIT + para::PER_BYTE * (32 + MAX_CODE_SIZE as u128);
	RegistrarPara::execute_with(|| assert_eq!(para_held(&ALICE), deposit));

	relay_from_para(ParaRequestV1::Deregister);

	Relay::execute_with(|| {
		assert!(!relay_has_record(para_id));
		assert!(relay_offboarding(para_id));
	});
	// The deposits are the manager's, so the manager gets them back.
	RegistrarPara::execute_with(|| {
		assert_eq!(para_state(para_id), None);
		assert_eq!(para_held(&ALICE), 0);
	});
}

#[test]
fn a_failing_relayed_request_is_reported_on_the_registrar_parachain() {
	MockNet::reset();

	let para_id = managed_para();

	// The para never produced a head, so there is no lock to lift.
	relay_from_para(ParaRequestV1::RemoveLock);

	RegistrarPara::execute_with(|| {
		assert_eq!(para_lock(para_id), None);
		assert!(para_event(|e| *e ==
			pallet_registrar_para::Event::RelayedRequest {
				para_id,
				result: Err(pallet_registrar_para::Error::<para::Runtime>::NotLocked.into()),
			}));
	});
}

#[test]
fn only_a_parachain_may_relay() {
	MockNet::reset();

	Relay::execute_with(|| {
		assert_noop!(
			relay::RegistrarRelay::relay(
				relay::RuntimeOrigin::signed(ALICE),
				ParaRequest::V1(ParaRequestV1::RemoveLock),
			),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn a_para_with_a_channel_unlocks_itself_directly() {
	MockNet::reset();

	let para_id = managed_para();
	note_head_on_relay(para_id);

	let call = ParaRuntimePallets::Registrar(RegistrarParaCalls::RemoveLock(para_id)).encode();
	ManagedPara::execute_with(|| {
		assert_ok!(send_xcm::<para::XcmRouter>(
			Location::new(1, [Parachain(PARA_ID)]),
			native_transact(call)
		));
	});

	RegistrarPara::execute_with(|| {
		assert_eq!(para_lock(para_id), Some(false));
		assert!(!para_event(|e| matches!(e, pallet_registrar_para::Event::RelayedRequest { .. })));
	});
	Relay::execute_with(|| {
		assert!(!relay_event(|e| matches!(e, pallet_registrar_relay::Event::Relayed { .. })));
	});
}
