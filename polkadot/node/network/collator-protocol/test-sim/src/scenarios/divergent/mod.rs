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

//! Catalog of intentional divergences between `LegacyValidator` and
//! `ExperimentalValidator`. Every test in this module must either:
//!
//! 1. Be a *paired* legacy/experimental test asserting each impl's actual contract
//!    (e.g. `*__legacy` asserts a bus event; `*__experimental` asserts the absence of
//!    that event), or
//! 2. Be `only = "legacy"` / `only = "experimental"` because the underlying capability
//!    only exists on one side.
//!
//! The point of carving these out of the shared regression suite is *legibility*: a
//! reviewer who wants to verify that the spec deviations are intentional reads this
//! directory. A reviewer who wants to verify regression-safety reads
//! `scenarios::*` — and there, both impls must agree.
//!
//! # Index
//!
//! | File | Divergence | Both impls? |
//! |------|------------|-------------|
//! | [`reputation_emission`] | Legacy emits `NetworkBridgeTx::ReportPeer`; experimental updates a persistent rep store and emits no bus event. | paired |
//! | [`reputation_behavior`] | Experimental's reputation has *behavioral* consequences (fetch ranking, 300ms penalty box for fresh peers, score-based slot eviction). Legacy fires-and-forgets to the bridge with no behavioral feedback. | experimental-only |
//! | [`no_time_based_eviction`] | Legacy disconnects undeclared / inactive peers via a time-based eviction policy. Experimental keeps them indefinitely; rep-pressure replaces the timer. Per RFC #616. | paired |
//! | [`upcoming_pr_11967`] | Tests for invariants that PR #11967 (rotation bug fix + capacity refactor) will introduce. Marked `bug_on = "experimental"` until merged. | experimental-only |
//! | [`upcoming_pr_12004`] | Tests for invariants that PR #12004 (avoid duplicate fetches) will introduce. Marked `bug_on = "experimental"` until merged. | experimental-only |
//!
//! # Adding a divergence
//!
//! 1. Confirm it is *intended*, not a bug. Bugs go in the regular scenario files marked
//!    with `#[sim_test(bug_on = "experimental", bug_url = "...")]`. If you can't decide
//!    yet, default to `bug_on` — moving from "known bug" to "intended divergence" is an
//!    explicit promotion that forces a second look.
//! 2. Cite the source (RFC, design doc, or PR). The module doc on the new file must
//!    explain *why* the divergence is intended.
//! 3. Pair the assertions: a `_legacy` and `_experimental` arm of the same scenario,
//!    using a shared setup helper, so the diff is *only* the assertion. No runtime
//!    `if Impl == Legacy` branches inside the test body — that obscures the spec.

#[cfg(test)]
pub(crate) mod no_time_based_eviction;
#[cfg(test)]
pub(crate) mod reputation_behavior;
#[cfg(test)]
pub(crate) mod reputation_emission;
#[cfg(test)]
pub(crate) mod upcoming_pr_11967;
#[cfg(test)]
pub(crate) mod upcoming_pr_11980;
#[cfg(test)]
pub(crate) mod upcoming_pr_12004;
