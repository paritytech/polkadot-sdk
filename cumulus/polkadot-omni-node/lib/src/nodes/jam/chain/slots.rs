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

//! JAM timeslots and wall-clock time.

use jam_interface::Slot as JamSlot;
use sp_timestamp::Timestamp;

pub(crate) const JAM_SLOT_DURATION_MS: u64 = 6000;

/// The JAM timeslot a wall-clock timestamp falls into — the inverse of
/// [`timestamp_for_anchor`](cumulus_jam_aura_primitives::timestamp_for_anchor).
///
/// The builder is driven by the wall clock, so it has to know which JAM slot it is in: that is
/// what tells it whether the JAM tip it caches is current.
pub(crate) fn jam_slot_at(timestamp: Timestamp) -> JamSlot {
	(timestamp.as_millis().saturating_sub(jam_types::JAM_COMMON_ERA * 1000) / JAM_SLOT_DURATION_MS)
		as JamSlot
}
