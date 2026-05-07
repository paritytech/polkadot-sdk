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
mod advertise_then_fetch;
#[cfg(test)]
mod bad_signature;
#[cfg(test)]
mod disconnect_if_no_declare;
#[cfg(test)]
mod disconnect_if_wrong_declare;
#[cfg(test)]
mod fetch_timeout;
#[cfg(test)]
mod full_seconding;
#[cfg(test)]
mod inactive_collator_eviction;
#[cfg(test)]
mod malicious_para;
#[cfg(test)]
pub(crate) mod shared;
#[cfg(test)]
mod unneeded_para;
