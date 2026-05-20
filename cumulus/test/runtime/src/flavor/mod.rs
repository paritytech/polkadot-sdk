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

//! Feature-gated parameters for [`crate::Runtime`].
//!
//! This crate is built many times with different Cargo features (see `Cargo.toml` and
//! `build.rs`). Each combination selects a **flavor** of the same runtime source: block
//! processing velocity, slot duration, relay parent offset, unincluded segment capacity, and
//! Aura backing behavior.
//!
//! Keep flavor-specific `#[cfg(...)]` ladders in the submodules here instead of in `lib.rs`, so
//! adding a new flavor stays localized and reviewable.

mod aura;
mod block_velocity;
mod relay_chain;
mod relay_parent;
mod slot;
mod unincluded_segment;

pub use aura::AllowMultipleBlocksPerSlot;
pub use block_velocity::BLOCK_PROCESSING_VELOCITY;
pub use relay_chain::RELAY_CHAIN_SLOT_DURATION_MILLIS;
pub use relay_parent::RELAY_PARENT_OFFSET;
pub use slot::SLOT_DURATION;
pub use unincluded_segment::UNINCLUDED_SEGMENT_CAPACITY;
