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

//! The rules a tick's timer and turn follow: the slot boundary, one turn per anchor slot, the
//! package budget, and a fresh tip.

use crate::nodes::jam::builder::parent::ParentSource;
use jam_interface::{CoreIndex, Slot as JamSlot};
use std::time::Duration;

/// How far the cached JAM tip may lag the wall clock before a tick is skipped.
///
/// Notification driving could not build on a stalled JAM chain — no notification, no block. A
/// wall-clock timer keeps ticking regardless, so it needs to be told when the tip it caches went
/// stale (JAM stalled, or the subscription died silently) rather than anchoring against it.
pub(crate) const MAX_TIP_LAG_SLOTS: JamSlot = 2;

/// How long until the next para slot starts.
///
/// Mirrors the relay slot-based collator's timer (`slot_based/slot_timer.rs`): slots are aligned
/// to the unix epoch, so all collators tick on the same boundaries.
pub(crate) fn time_until_next_para_slot(now: Duration, slot_duration: Duration) -> Duration {
	let now = now.as_millis();
	let slot_millis = slot_duration.as_millis().max(1);
	let next_slot_start = ((now + slot_millis) / slot_millis) * slot_millis;
	Duration::from_millis((next_slot_start - now) as u64)
}

/// One turn per anchor slot: the Aura guard, on the anchor slot rather than the wall clock.
///
/// The turn is what claims the slot; the blocks inside it are the turn's packages, one per core,
/// and they deliberately claim the same Aura slot. Only a second *turn* in the same anchor slot
/// would be equivocation, and that is what the guard refuses.
pub(crate) fn anchor_slot_claimed(last_claimed: Option<JamSlot>, anchor: JamSlot) -> bool {
	last_claimed.is_some_and(|last| anchor <= last)
}

/// How many packages one turn may author, before the runtime's gate is consulted.
///
/// One per core holding the para's authorizer, and never zero: with no core at all the turn
/// still authors its first package — the para keeps producing locally — it simply has nowhere
/// to submit it, so nothing is ever handed to a guarantor and the package never holds authoring.
pub(crate) fn turn_package_budget(cores: usize) -> usize {
	cores.max(1)
}

/// Whether the turn may author the `package`-th package.
///
/// The builder's half: a package after the first needs a core of its own, because a turn puts
/// one package per core so they land on different cores of the same JAM block. The runtime's
/// half is velocity and capacity — runtime-owned, unreadable here, and reached only through
/// `can_build_upon`, which is what ends the burst when they are spent.
pub(crate) fn package_fits_turn(
	package: usize,
	core: Option<CoreIndex>,
	runtime_admits: bool,
) -> bool {
	(package == 0 || core.is_some()) && runtime_admits
}

/// The source for a turn's package once the first one is authored.
///
/// A turn's packages chain, so every package after the first extends the branch the first one
/// started — whether that is the ordinary deepest branch or one a re-root opened. Both mappings
/// are idempotent, so this can be applied once per package.
pub(crate) fn next_parent_source(source: ParentSource) -> ParentSource {
	match source {
		ParentSource::Reroot | ParentSource::Rerooted => ParentSource::Rerooted,
		ParentSource::AccumulatedHead | ParentSource::Deepest => ParentSource::Deepest,
	}
}

/// Whether the cached JAM tip is recent enough to anchor against.
pub(crate) fn tip_is_fresh(wall_jam_slot: JamSlot, tip_slot: JamSlot) -> bool {
	wall_jam_slot.saturating_sub(tip_slot) <= MAX_TIP_LAG_SLOTS
}
