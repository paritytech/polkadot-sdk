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

//! Blocks processed per relay chain slot for this build flavor.

#[cfg(any(feature = "elastic-scaling-500ms", feature = "block-bundling"))]
pub const BLOCK_PROCESSING_VELOCITY: u32 = 12;

#[cfg(all(feature = "elastic-scaling-multi-block-slot", not(feature = "elastic-scaling-500ms")))]
pub const BLOCK_PROCESSING_VELOCITY: u32 = 6;

#[cfg(all(
	any(feature = "elastic-scaling", feature = "relay-parent-offset"),
	not(feature = "elastic-scaling-500ms"),
	not(feature = "elastic-scaling-multi-block-slot")
))]
pub const BLOCK_PROCESSING_VELOCITY: u32 = 3;

#[cfg(not(any(
	feature = "elastic-scaling",
	feature = "elastic-scaling-500ms",
	feature = "elastic-scaling-multi-block-slot",
	feature = "relay-parent-offset",
	feature = "block-bundling",
)))]
pub const BLOCK_PROCESSING_VELOCITY: u32 = 1;
