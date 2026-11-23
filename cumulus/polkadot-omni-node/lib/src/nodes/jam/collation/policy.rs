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
//! Two concerns live here:
//!
//! 1. **Package status policy** ([`ForgetWhenAnchorExpires`]): a pure strategy the collation
//!    manager calls on every JAM block and every status update to decide whether a package has been
//!    reported, should keep waiting, or should be forgotten.
//!
//! 2. **Resubmission trigger** ([`resubmission_due`] and [`original_package_expired`]): pure
//!    predicates the collation manager evaluates after the β handling to decide whether to start a
//!    resubmission round — rebuilding lost blocks into new work packages signed by this collator —
//!    and which blocks above the accumulated head such a round may rebuild.

use jam_interface::{Slot as JamSlot, WorkPackageStatus};

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
