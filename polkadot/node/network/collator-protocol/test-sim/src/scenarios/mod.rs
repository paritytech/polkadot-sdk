// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Vertical-slice scenarios: read each one as a spec.
//!
//! The slice covers the legacy validator side first. New scenarios in this module exercise:
//!
//! - bad-signature declarations get penalised (see [`bad_signature`]),
//! - declarations for an unassigned para get disconnected (see [`unneeded_para`]).
//!
//! Heavier flows (advertise → fetch → second) follow once the responder DSL grows the helpers
//! required to script the view-update query barrage.

#[cfg(test)]
mod activity_extends_life;
#[cfg(test)]
mod advertise_then_fetch;
#[cfg(test)]
mod advertisement_spam_protection;
#[cfg(test)]
mod bad_signature;
#[cfg(test)]
mod child_blocked_from_seconding_by_parent;
#[cfg(test)]
mod claim_queue_window;
#[cfg(test)]
mod claims_counting;
#[cfg(test)]
mod collation_fetching_considers_advertisements_from_the_whole_view;
#[cfg(test)]
mod collation_fetching_fairness_handles_old_claims;
#[cfg(test)]
mod collation_fetching_prefer_entries_earlier_in_claim_queue;
#[cfg(test)]
mod disconnect_if_no_declare;
#[cfg(test)]
mod disconnect_if_wrong_declare;
#[cfg(test)]
mod fair_collation_fetches;
#[cfg(test)]
mod fetch_next_on_invalid;
#[cfg(test)]
mod fetches_next_collation;
#[cfg(test)]
mod fetch_timeout;
#[cfg(test)]
mod fragment_chain_seconding;
#[cfg(test)]
mod full_seconding;
#[cfg(test)]
mod group_rotation_uses_correct_core_per_relay_parent;
#[cfg(test)]
mod inactive_collator_eviction;
#[cfg(test)]
mod malicious_para;
#[cfg(test)]
mod peer_disconnect_clears_queue;
#[cfg(test)]
mod response_sanity_check;
#[cfg(test)]
mod second_multiple_candidates_per_relay_parent;
#[cfg(test)]
pub(crate) mod shared;
#[cfg(test)]
pub(crate) mod world;
#[cfg(test)]
mod single_fetch_per_relay_parent;
#[cfg(test)]
mod unneeded_para;
#[cfg(test)]
mod v3_scheduling_parent;
#[cfg(test)]
mod v3_session_index_checks;
#[cfg(test)]
mod v1_advertise_on_non_leaf;
#[cfg(test)]
mod v1_descriptor_version_detection_with_v3_enabled;
#[cfg(test)]
mod v1_full_seconding_with_back_notification;
#[cfg(test)]
mod view_change_disconnects;
