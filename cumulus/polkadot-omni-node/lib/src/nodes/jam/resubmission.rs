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

//! Work-package lifecycle policy and resubmission decision logic.
//!
//! Three concerns live here:
//!
//! 1. **Package status policy** ([`ForgetWhenAnchorExpires`]): a pure strategy the collation
//!    manager calls on every JAM block and every status update to decide whether a package has been
//!    reported, should keep waiting, or should be forgotten.
//!
//! 2. **Resubmission trigger** ([`resubmission_due`] and [`original_package_expired`]): pure
//!    predicates the collation manager evaluates after the β handling to decide whether to start a
//!    resubmission round — rebuilding lost blocks into new work packages signed by this collator —
//!    and which blocks above the accumulated head such a round may rebuild.
//!
//! 3. **Header chain walk** ([`header_chain_until`]): an async helper that fetches JAM headers
//!    starting at the tip, following each header's parent, until every hash the resubmission round
//!    needs has been seen or the chain length cap is reached.

use cumulus_primitives_core::jam_anchor_proof::{
	jam_header_hash, JamHeaderPrefix, MAX_ANCHOR_CHAIN_LEN,
};
use jam_interface::{HeaderHash, Slot as JamSlot, WorkPackageStatus};
use std::{collections::HashSet, future::Future};

/// How long a package has to be reported, counted from its anchor: the anchor must still be in
/// JAM's recent history when the package is reported. Past this the package is forgotten, and its
/// block — still in the local database and the PoV store — waits for a resubmission round to
/// rebuild it into a new package.
pub(crate) const REPORT_DEADLINE_SLOTS: JamSlot = 8;

/// How many JAM slots the accumulated head must stand still (since the later of the last head
/// change and the last resubmission round) before a resubmission round fires.
pub(crate) const RESUBMIT_STALL_SLOTS: JamSlot = 3;

/// At most this many blocks are resubmitted in one round.
pub(crate) const MAX_RESUBMIT_BLOCKS: usize = 4;

/// What the collation manager should do with an in-flight work package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyAction {
	/// Keep waiting for further status updates.
	Wait,
	/// The package was reported; stop watching the clock on it. It stays tracked until the para
	/// head shows its height settled.
	Reported,
	/// Give up on the package. Nothing else has to be undone — no other package depends on it.
	Forget,
}

/// What the manager saw about one package when a JAM block arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Observed {
	/// The JAM slot of the block that just arrived.
	pub tip_slot: JamSlot,
	/// The JAM slot of the package's anchor, the start of its report window.
	pub anchor_slot: JamSlot,
	/// Whether β already names the package's hash.
	pub on_chain: bool,
}

pub(crate) trait ResubmissionPolicy: Send {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction;

	/// A new JAM block arrived: the package either appears in β or its anchor's age decides its
	/// fate.
	fn on_jam_block(&self, observed: Observed) -> PolicyAction;
}

/// Wait for a report, and give up once the package's anchor expires.
///
/// The anchor deadline is the only clock: a package is forgotten exactly when the status tracker
/// would also fail it (`anchor + REPORT_DEADLINE_SLOTS` blocks).
pub(crate) struct ForgetWhenAnchorExpires;

impl ResubmissionPolicy for ForgetWhenAnchorExpires {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction {
		match status {
			WorkPackageStatus::Reportable { .. } => PolicyAction::Wait,
			WorkPackageStatus::Reported { .. } | WorkPackageStatus::Ready { .. } => {
				PolicyAction::Reported
			},
			WorkPackageStatus::Failed(_) => PolicyAction::Forget,
		}
	}

	fn on_jam_block(&self, observed: Observed) -> PolicyAction {
		if observed.on_chain {
			return PolicyAction::Reported;
		}
		if observed.tip_slot.saturating_sub(observed.anchor_slot) >= REPORT_DEADLINE_SLOTS {
			PolicyAction::Forget
		} else {
			PolicyAction::Wait
		}
	}
}

/// Whether a resubmission round should start now.
///
/// All inputs are pre-computed by the manager; the function is pure so every combination is
/// unit-testable without a chain.
pub(crate) fn resubmission_due(
	tip_slot: JamSlot,
	head_changed_at: JamSlot,
	last_round_at: Option<JamSlot>,
	has_unaccumulated_child: bool,
	our_turn: bool,
	round_in_flight: bool,
) -> bool {
	our_turn &&
		has_unaccumulated_child &&
		!round_in_flight &&
		tip_slot.saturating_sub(head_changed_at.max(last_round_at.unwrap_or(0))) >=
			RESUBMIT_STALL_SLOTS
}

/// Whether a block's original package can no longer be reported at `tip_slot`: the anchor it was
/// built against — the block's `JamParent` anchor, which is also that package's anchor — is past
/// the report deadline, so the package is forgotten (or never existed, for a block another
/// collator authored and lost).
///
/// Only such a block is rebuilt. A younger one may still land in the package that first carried
/// it: on a slow chain report and accumulate alone can take longer than the stall a round waits
/// for, and rebuilding then would put a second package for the same block on a core.
pub(crate) fn original_package_expired(tip_slot: JamSlot, anchor_slot: JamSlot) -> bool {
	tip_slot.saturating_sub(anchor_slot) >= REPORT_DEADLINE_SLOTS
}

/// The result of walking the JAM header chain from the tip backward.
pub(crate) struct HeaderChain {
	/// The encoded headers, newest first (starting at the tip's header).
	pub headers: Vec<Vec<u8>>,
	/// The hash of each entry of `headers`, in the same order.
	pub hashes: Vec<HeaderHash>,
	/// Hashes that were requested but not found within [`MAX_ANCHOR_CHAIN_LEN`] headers.
	pub missing: HashSet<HeaderHash>,
}

impl HeaderChain {
	/// The prefix of the chain, newest first, that ends at the oldest of `hashes`: the shortest
	/// chain starting at the tip that proves every one of them. `None` when one of `hashes` is not
	/// in the chain.
	pub fn prefix_reaching(&self, hashes: &[HeaderHash]) -> Option<&[Vec<u8>]> {
		let oldest = hashes
			.iter()
			.map(|hash| self.hashes.iter().position(|known| known == hash))
			.collect::<Option<Vec<_>>>()?
			.into_iter()
			.max()?;
		Some(&self.headers[..=oldest])
	}
}

/// Walk the JAM header chain from `start` backward, collecting encoded headers until every hash
/// in `needed` has been seen or [`MAX_ANCHOR_CHAIN_LEN`] headers have been fetched.
///
/// `fetch` returns the encoded header bytes for a given hash; bytes that do not hash to the hash
/// they were fetched for end the walk with an error, since a chain built from them would fail the
/// PVF's proof. The walk parses only the fixed-size prefix to find each header's parent; the full
/// encoding is kept for the PVF's anchor proof.
pub(crate) async fn header_chain_until<F, Fut>(
	start: HeaderHash,
	needed: HashSet<HeaderHash>,
	fetch: F,
) -> Result<HeaderChain, String>
where
	F: Fn(HeaderHash) -> Fut,
	Fut: Future<Output = Result<Vec<u8>, String>>,
{
	let mut headers = Vec::new();
	let mut hashes = Vec::new();
	let mut remaining = needed;
	let mut current = start;

	for _ in 0..MAX_ANCHOR_CHAIN_LEN {
		let encoded = fetch(current).await?;
		let actual = HeaderHash(jam_header_hash(&encoded).0);
		if actual != current {
			return Err(format!("header {current:?}: the node served bytes hashing to {actual:?}"));
		}
		let prefix = JamHeaderPrefix::parse(&encoded).ok_or_else(|| {
			format!("header {current:?} is shorter than the fixed prefix ({} bytes)", encoded.len())
		})?;
		headers.push(encoded);
		hashes.push(current);
		remaining.remove(&current);

		if remaining.is_empty() {
			break;
		}
		current = HeaderHash(prefix.parent.0);
	}

	Ok(HeaderChain { headers, hashes, missing: remaining })
}

#[cfg(test)]
mod tests {
	use super::*;
	use jam_interface::BlockDesc;
	use jam_types::{CoreIndex, StateRootHash, WorkReportHash};

	fn reportable(remaining_blocks: u16) -> WorkPackageStatus {
		WorkPackageStatus::Reportable { remaining_blocks }
	}

	fn reported() -> WorkPackageStatus {
		WorkPackageStatus::Reported {
			reported_in: BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 1 },
			core: CoreIndex::default(),
			report_hash: WorkReportHash::from([2u8; 32]),
		}
	}

	fn observed(tip_slot: JamSlot, anchor_slot: JamSlot, on_chain: bool) -> Observed {
		Observed { tip_slot, anchor_slot, on_chain }
	}

	#[test]
	fn waits_while_reportable_and_finishes_on_reported() {
		let policy = ForgetWhenAnchorExpires;
		assert_eq!(policy.on_status(&reportable(8)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reportable(3)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reported()), PolicyAction::Reported);
	}

	/// `Ready` is still in flight — queued for accumulation, not accumulated — so it is treated
	/// exactly like `Reported`: the deadline stops applying and the para head decides completion.
	#[test]
	fn a_ready_package_is_treated_as_reported() {
		let policy = ForgetWhenAnchorExpires;
		let ready = WorkPackageStatus::Ready {
			reported_in: BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 1 },
			core: CoreIndex::default(),
			report_hash: WorkReportHash::from([2u8; 32]),
			ready_in: BlockDesc { header_hash: HeaderHash::from([3u8; 32]), slot: 2 },
		};
		assert_eq!(policy.on_status(&ready), PolicyAction::Reported);
	}

	/// A failure — "Not reported in time" or a spent anchor — is terminal: the package is
	/// forgotten, and its block waits for a resubmission round to rebuild it into a new package.
	#[test]
	fn a_failure_is_forgotten() {
		let policy = ForgetWhenAnchorExpires;
		let failed = WorkPackageStatus::Failed("anchor expired".into());
		assert_eq!(policy.on_status(&failed), PolicyAction::Forget);
	}

	/// β not naming the package is no reason to act while its anchor can still be reported:
	/// nothing is ever sent twice, so the silence is waited out, however long it lasts.
	#[test]
	fn silence_is_waited_out_while_the_anchor_lives() {
		let policy = ForgetWhenAnchorExpires;
		assert_eq!(policy.on_jam_block(observed(100, 100, false)), PolicyAction::Wait);
		assert_eq!(
			policy.on_jam_block(observed(100 + REPORT_DEADLINE_SLOTS - 1, 100, false)),
			PolicyAction::Wait,
		);
	}

	/// At `anchor + REPORT_DEADLINE_SLOTS` the package can never be reported, so it is forgotten;
	/// one slot earlier it still waits.
	#[test]
	fn an_expired_anchor_is_forgotten() {
		let policy = ForgetWhenAnchorExpires;
		let at = |tip: JamSlot| observed(tip, 0, false);

		assert_eq!(policy.on_jam_block(at(REPORT_DEADLINE_SLOTS - 1)), PolicyAction::Wait);
		assert_eq!(policy.on_jam_block(at(REPORT_DEADLINE_SLOTS)), PolicyAction::Forget);
	}

	/// Appearing in β is the truth, whatever the deadline says: a reported package is reported
	/// even when its anchor has expired.
	#[test]
	fn appearing_on_chain_wins_over_the_deadline() {
		let policy = ForgetWhenAnchorExpires;
		let expired = observed(100, 0, true);
		assert_eq!(policy.on_jam_block(expired), PolicyAction::Reported);
	}

	// ---- resubmission_due tests ----

	#[test]
	fn resubmission_fires_when_all_conditions_hold() {
		assert!(resubmission_due(
			10,    // tip_slot
			7,     // head_changed_at
			None,  // last_round_at
			true,  // has_unaccumulated_child
			true,  // our_turn
			false, // round_in_flight
		));
	}

	#[test]
	fn resubmission_blocked_when_not_our_turn() {
		assert!(!resubmission_due(10, 7, None, true, false, false));
	}

	#[test]
	fn resubmission_blocked_when_round_in_flight() {
		assert!(!resubmission_due(10, 7, None, true, true, true));
	}

	#[test]
	fn resubmission_blocked_when_no_unaccumulated_child() {
		assert!(!resubmission_due(10, 7, None, false, true, false));
	}

	#[test]
	fn resubmission_blocked_when_stall_too_short() {
		assert!(!resubmission_due(9, 7, None, true, true, false));
	}

	#[test]
	fn last_round_at_extends_the_stall_clock() {
		// head changed at 5, last round at 8 → stall measured from 8
		assert!(!resubmission_due(10, 5, Some(8), true, true, false));
		assert!(resubmission_due(11, 5, Some(8), true, true, false));
	}

	#[test]
	fn head_change_resets_the_stall_clock() {
		// head changed at 9, no prior round → stall measured from 9
		assert!(!resubmission_due(11, 9, None, true, true, false));
		assert!(resubmission_due(12, 9, None, true, true, false));
	}

	// ---- original_package_expired tests ----

	/// A block is rebuildable exactly from the slot its original package is forgotten at: one slot
	/// before the deadline the package may still be reported, however long the head has stalled.
	#[test]
	fn a_block_is_rebuildable_once_its_original_package_has_expired() {
		let anchor_slot = 100;
		assert!(!original_package_expired(anchor_slot, anchor_slot));
		assert!(!original_package_expired(anchor_slot + REPORT_DEADLINE_SLOTS - 1, anchor_slot));
		assert!(original_package_expired(anchor_slot + REPORT_DEADLINE_SLOTS, anchor_slot));
		assert!(original_package_expired(anchor_slot + REPORT_DEADLINE_SLOTS + 50, anchor_slot));
	}

	/// The deadline the gate applies is the one the status policy forgets the package by, so the
	/// two can never disagree about a block of this collator's own.
	#[test]
	fn the_rebuild_gate_agrees_with_the_forget_deadline() {
		let policy = ForgetWhenAnchorExpires;
		for tip in 0..3 * REPORT_DEADLINE_SLOTS {
			let forgotten = policy.on_jam_block(observed(tip, REPORT_DEADLINE_SLOTS, false)) ==
				PolicyAction::Forget;
			assert_eq!(
				original_package_expired(tip, REPORT_DEADLINE_SLOTS),
				forgotten,
				"tip {tip}"
			);
		}
	}

	// ---- header_chain_until tests ----

	use sp_core::H256;

	/// Build a synthetic encoded header: 100-byte prefix + 200-byte tail.
	fn synthetic_header(parent: HeaderHash, state_root: H256, slot: u32) -> Vec<u8> {
		let mut bytes = vec![0u8; 300];
		bytes[0..32].copy_from_slice(&parent.0);
		bytes[32..64].copy_from_slice(state_root.as_bytes());
		// 64..96 = extrinsic hash (zeros)
		bytes[96..100].copy_from_slice(&slot.to_le_bytes());
		bytes
	}

	/// Build a chain of `len` synthetic headers, newest first. Returns (headers, hashes).
	fn build_chain(len: usize, start_slot: u32) -> (Vec<Vec<u8>>, Vec<HeaderHash>) {
		let mut headers = Vec::new();
		let mut hashes = Vec::new();
		// Build oldest first, then reverse.
		let mut parent = HeaderHash::from([0xFFu8; 32]);
		let mut chain: Vec<(Vec<u8>, HeaderHash)> = Vec::new();
		for i in 0..len {
			let slot = start_slot + i as u32;
			let encoded = synthetic_header(parent, H256::repeat_byte((i + 1) as u8), slot);
			let hash = HeaderHash(jam_header_hash(&encoded).0);
			chain.push((encoded, hash));
			parent = hash;
		}
		chain.reverse();
		for (enc, hash) in chain {
			headers.push(enc);
			hashes.push(hash);
		}
		(headers, hashes)
	}

	#[tokio::test]
	async fn header_chain_finds_all_needed_hashes() {
		let (headers, hashes) = build_chain(5, 10);
		// Map: hash -> encoded header
		let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
			hashes.iter().cloned().zip(headers.iter().cloned()).collect();

		let needed: HashSet<HeaderHash> = [hashes[0], hashes[2], hashes[4]].into();
		let result = header_chain_until(hashes[0], needed, |h| {
			let m = map.clone();
			async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
		})
		.await
		.expect("the walk succeeds");

		assert!(result.missing.is_empty(), "all needed hashes found");
		assert_eq!(result.headers.len(), 5, "walked the full chain to the oldest needed");
		assert_eq!(result.hashes, hashes, "each header is filed under the hash it was fetched for");
	}

	/// One walk serves a whole round: the chain reaching every block's anchors is cut per package
	/// to the oldest hash that package needs, and a hash the walk never saw cuts nothing.
	#[tokio::test]
	async fn a_chain_is_cut_to_the_oldest_hash_a_package_needs() {
		let (headers, hashes) = build_chain(6, 1);
		let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
			hashes.iter().cloned().zip(headers.iter().cloned()).collect();
		let needed: HashSet<HeaderHash> = [hashes[1], hashes[4]].into();
		let chain = header_chain_until(hashes[0], needed, |h| {
			let m = map.clone();
			async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
		})
		.await
		.expect("the walk succeeds");
		assert_eq!(chain.headers.len(), 5, "the walk stopped at the oldest needed hash");

		assert_eq!(chain.prefix_reaching(&[hashes[1], hashes[0]]), Some(&headers[..=1]));
		assert_eq!(chain.prefix_reaching(&[hashes[2], hashes[4]]), Some(&headers[..=4]));
		assert_eq!(chain.prefix_reaching(&[hashes[4], hashes[1]]), Some(&headers[..=4]));
		assert_eq!(chain.prefix_reaching(&[hashes[1], hashes[5]]), None, "never walked to");
		assert_eq!(chain.prefix_reaching(&[]), None, "nothing to reach");
	}

	/// Bytes that do not hash to the header they were fetched as cannot prove anything: the walk
	/// fails on them instead of handing the PVF a chain it will reject.
	#[tokio::test]
	async fn header_chain_rejects_bytes_that_do_not_hash_to_the_requested_header() {
		let (headers, hashes) = build_chain(2, 1);
		let needed: HashSet<HeaderHash> = [hashes[1]].into();
		let result = header_chain_until(hashes[0], needed, |_| {
			let wrong = headers[1].clone();
			async move { Ok::<_, String>(wrong) }
		})
		.await;

		let error = result.err().expect("the walk fails");
		assert!(error.contains(&format!("{:?}", hashes[0])), "{error}");
	}

	#[tokio::test]
	async fn header_chain_reports_missing_when_beyond_cap() {
		// Build a chain longer than MAX_ANCHOR_CHAIN_LEN and request a hash past the cap.
		let (headers, hashes) = build_chain(MAX_ANCHOR_CHAIN_LEN + 5, 1);
		let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
			hashes.iter().cloned().zip(headers.iter().cloned()).collect();

		let far_hash = hashes[MAX_ANCHOR_CHAIN_LEN + 2];
		let needed: HashSet<HeaderHash> = [hashes[0], far_hash].into();
		let result = header_chain_until(hashes[0], needed, |h| {
			let m = map.clone();
			async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
		})
		.await
		.expect("the walk succeeds");

		assert!(result.missing.contains(&far_hash), "the distant hash is missing");
		assert_eq!(result.headers.len(), MAX_ANCHOR_CHAIN_LEN);
	}

	#[tokio::test]
	async fn header_chain_stops_at_oldest_needed() {
		let (headers, hashes) = build_chain(10, 1);
		let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
			hashes.iter().cloned().zip(headers.iter().cloned()).collect();

		// Only need the first and third header — should stop at the third.
		let needed: HashSet<HeaderHash> = [hashes[0], hashes[2]].into();
		let result = header_chain_until(hashes[0], needed, |h| {
			let m = map.clone();
			async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
		})
		.await
		.expect("the walk succeeds");

		assert!(result.missing.is_empty());
		assert_eq!(result.headers.len(), 3, "stopped at the oldest needed hash");
	}

	#[tokio::test]
	async fn header_chain_propagates_fetch_error() {
		let needed: HashSet<HeaderHash> = [HeaderHash::from([1u8; 32])].into();
		let result = header_chain_until(HeaderHash::from([1u8; 32]), needed, |_| async {
			Err::<Vec<u8>, String>("node unreachable".into())
		})
		.await;

		assert!(result.is_err());
	}

	/// Pins [`JamHeaderPrefix`] and [`jam_header_hash`] to polkajam's real header encoding, which
	/// the synthetic headers above only imitate.
	#[test]
	fn header_prefix_and_hash_match_polkajam_header_encoding() {
		let mut header = jam_std_common::Header::genesis();
		header.parent = HeaderHash::from([0xABu8; 32]);
		header.parent_state_root = StateRootHash::from([0xCDu8; 32]);
		header.slot = 0x0403_0201;
		let encoded = jam_codec::Encode::encode(&header);

		let prefix = JamHeaderPrefix::parse(&encoded).expect("a real header is long enough");
		assert_eq!(prefix.parent.0, header.parent.0);
		assert_eq!(prefix.parent_state_root.0, header.parent_state_root.0);
		assert_eq!(prefix.slot, header.slot);
		assert_eq!(HeaderHash(jam_header_hash(&encoded).0), header.hash());
	}
}
