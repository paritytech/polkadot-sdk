// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The JAM code-upgrade path of [`crate::Pallet`].
//!
//! On JAM there is no relay-chain `UpgradeGoAhead` signal and no on-chain code bytes: the code
//! lives offchain and is addressed by hash. Each block reconciles the locally scheduled
//! [`PendingCodeHash`] with the parachain's `ParaInfo` from the carried JAM state proof:
//!
//! - once the service's active `validation_code` matches, the upgrade is already live;
//! - once the service's `announced_upgrade` matches, the switch is immediate, so the 40-byte marker
//!   is written to `:code` and an `Apply` is emitted for the service;
//! - otherwise the code is announced to the service and the local pending state stays armed.
//!
//! [`decide_upgrade`] is pure, so the decision is testable without JAM state;
//! [`apply_if_ready_now`] carries it out.

use crate::{Config, OnCodeApplied, PendingCodeHash, PendingUpgradeEmit};
use parachain_service_core::{types::ValidationCodeRef, CodeUpgradePhase, ParaInfo};

/// Decision returned by [`decide_upgrade`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum JamUpgradeDecision {
	/// The service already runs the pending code; the local marker is (re-)written, no upward
	/// message is needed.
	Applied,
	/// The service has announced the pending code; the switch is immediate, so the marker is
	/// written and an `Apply` is emitted for the service.
	EmitApply,
	/// The service has not seen the code; announce it and keep the local pending state armed.
	EmitAnnouncement,
}

/// The phase of a pending upgrade emission, mirroring [`CodeUpgradePhase`].
///
/// FRAME generates storage metadata for [`crate::PendingUpgradeEmit`], which requires `TypeInfo`;
/// the shared ABI enum does not derive it, so the storage holds this local twin and
/// `jam_validate_block` converts it when it sends the message.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	codec::Encode,
	codec::Decode,
	scale_info::TypeInfo,
	codec::MaxEncodedLen,
)]
pub enum EmitPhase {
	Announcement,
	Apply,
}

impl From<EmitPhase> for CodeUpgradePhase {
	fn from(phase: EmitPhase) -> Self {
		match phase {
			EmitPhase::Announcement => CodeUpgradePhase::Announcement,
			EmitPhase::Apply => CodeUpgradePhase::Apply,
		}
	}
}

/// Decide the JAM upgrade from the scheduled code and the service's `ParaInfo`.
///
/// Only hash/len are compared against `ParaInfo`, never code bytes. A missing `ParaInfo` (the
/// service has not registered or stored anything yet) is treated as "not seen", so the code is
/// announced rather than silently dropped.
pub(crate) fn decide_upgrade(
	pending: ([u8; 32], u32),
	para_info: Option<&ParaInfo>,
) -> JamUpgradeDecision {
	let (hash, len) = pending;

	if let Some(info) = para_info {
		if info
			.validation_code
			.as_ref()
			.is_some_and(|active| code_ref_matches(active, hash, len))
		{
			return JamUpgradeDecision::Applied;
		}

		if info
			.announced_upgrade
			.as_ref()
			.is_some_and(|announced| code_ref_matches(announced, hash, len))
		{
			return JamUpgradeDecision::EmitApply;
		}
	}

	JamUpgradeDecision::EmitAnnouncement
}

/// Reconcile the pending upgrade, if any, with the service's `para_info`.
///
/// When the service runs or has announced the pending code, the code is applied: the code marker
/// `sp_code_marker::encode_marker(&hash)` is written to `:code`,
/// `DigestItem::RuntimeEnvironmentUpdated` is deposited, [`PendingCodeHash`] is cleared and
/// [`Config::OnCodeApplied`] is called. The upward message the decision implies goes to
/// [`PendingUpgradeEmit`], for `jam_validate_block` to send after execution.
pub(crate) fn apply_if_ready_now<T: Config>(para_info: Option<&ParaInfo>) {
	use frame_support::storage::unhashed;
	use sp_core::storage::well_known_keys;

	let Some((hash, len)) = PendingCodeHash::<T>::get() else { return };

	let decision = decide_upgrade((hash, len), para_info);

	if matches!(decision, JamUpgradeDecision::Applied | JamUpgradeDecision::EmitApply) {
		let marker = sp_code_marker::encode_marker(&hash);
		// We write :code directly rather than going through
		// frame_system::update_code_in_storage (which would carry multi-MB bytes and
		// deposit the digest for us). Writing directly bypasses that deposit, so we
		// do it ourselves — wait_for_runtime_upgrade in the zombienet helpers detects
		// exactly this digest to confirm the upgrade completed.
		unhashed::put_raw(well_known_keys::CODE, &marker);
		frame_system::Pallet::<T>::deposit_log(
			sp_runtime::generic::DigestItem::RuntimeEnvironmentUpdated,
		);
		PendingCodeHash::<T>::kill();
		T::OnCodeApplied::on_code_applied();
	}

	let phase = match decision {
		JamUpgradeDecision::Applied => return,
		JamUpgradeDecision::EmitApply => EmitPhase::Apply,
		JamUpgradeDecision::EmitAnnouncement => EmitPhase::Announcement,
	};
	PendingUpgradeEmit::<T>::put((hash, len, phase));
}

/// Whether a scheduled code length fits the JAM preimage limit: the largest preimage the network
/// will carry, [`parachain_service_core::MAX_VALIDATION_CODE_SIZE`].
pub(crate) fn code_size_fits(len: usize) -> bool {
	len <= parachain_service_core::MAX_VALIDATION_CODE_SIZE as usize
}

fn code_ref_matches(code: &ValidationCodeRef, hash: [u8; 32], len: u32) -> bool {
	code.hash == hash.into() && code.len == len
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;
	use parachain_service_core::types::HeadData;

	fn code_ref(hash: [u8; 32], len: u32) -> ValidationCodeRef {
		ValidationCodeRef { hash: hash.into(), len }
	}

	fn para_info(
		active: Option<ValidationCodeRef>,
		announced: Option<ValidationCodeRef>,
	) -> ParaInfo {
		ParaInfo {
			head_data: HeadData::try_from(vec![0xca, 0xfe]).expect("2 bytes < 4 KiB; qed"),
			validation_code: active,
			announced_upgrade: announced,
			total_state_balance: 0,
			used_state_balance: 0,
			is_deregistering: false,
		}
	}

	/// The service already runs the code: nothing to emit.
	#[test]
	fn active_code_is_applied() {
		let hash = [1u8; 32];
		let len = 3u32;
		let info = para_info(Some(code_ref(hash, len)), None);
		assert_eq!(decide_upgrade((hash, len), Some(&info)), JamUpgradeDecision::Applied);
	}

	/// The service announced the code: the switch is immediate, so emit `Apply`.
	#[test]
	fn announced_upgrade_emits_apply() {
		let hash = [1u8; 32];
		let len = 3u32;
		let info = para_info(None, Some(code_ref(hash, len)));
		assert_eq!(decide_upgrade((hash, len), Some(&info)), JamUpgradeDecision::EmitApply);
	}

	/// A different active code does not apply; a different announcement does not either.
	#[test]
	fn mismatched_refs_emit_announcement() {
		let hash = [1u8; 32];
		let len = 3u32;

		let wrong_len = para_info(Some(code_ref(hash, 4)), None);
		assert_eq!(
			decide_upgrade((hash, len), Some(&wrong_len)),
			JamUpgradeDecision::EmitAnnouncement
		);

		let wrong_hash = para_info(None, Some(code_ref([0xaa; 32], len)));
		assert_eq!(
			decide_upgrade((hash, len), Some(&wrong_hash)),
			JamUpgradeDecision::EmitAnnouncement
		);
	}

	/// No `ParaInfo` at all: announce rather than silently drop.
	#[test]
	fn missing_para_info_emits_announcement() {
		let hash = [1u8; 32];
		let len = 3u32;
		assert_eq!(decide_upgrade((hash, len), None), JamUpgradeDecision::EmitAnnouncement);
	}

	#[test]
	fn code_at_limit_fits() {
		assert!(code_size_fits(parachain_service_core::MAX_VALIDATION_CODE_SIZE as usize));
	}

	#[test]
	fn code_over_limit_does_not_fit() {
		assert!(!code_size_fits(parachain_service_core::MAX_VALIDATION_CODE_SIZE as usize + 1));
	}
}
