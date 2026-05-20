// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
//
// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Aura / timestamp slot duration for this build flavor (milliseconds).

#[cfg(feature = "slot-duration-18s")]
pub const SLOT_DURATION: u64 = 18000;

#[cfg(all(
	any(feature = "sync-backing", feature = "elastic-scaling-12s-slot"),
	not(feature = "slot-duration-18s")
))]
pub const SLOT_DURATION: u64 = 12000;

#[cfg(not(any(
	feature = "sync-backing",
	feature = "elastic-scaling-12s-slot",
	feature = "slot-duration-18s"
)))]
pub const SLOT_DURATION: u64 = 6000;
