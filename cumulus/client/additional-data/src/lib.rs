// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Cumulus-specific relay-state providers for the dynamic `read_relay_chain_state` mechanism. Each
//! implements both
//! [`RelayStateReader`](cumulus_primitives_additional_data::RelayStateReader) (the read host
//! function) and [`AdditionalDataFinalizer`](sp_additional_data::AdditionalDataFinalizer) (the
//! additional-data digest), so one instance backs both the `RelayStateExt` and `AdditionalDataExt`
//! extensions.
//!
//! - [`RecordingAdditionalDataProvider`]: build side — records the relay state a runtime reads
//!   while authoring, and assembles the additional-data map carried in the PoV.
//! - [`VerifyingAdditionalDataProvider`]: **generic block-import** side (full-node re-execution) —
//!   serves reads back from the proof carried in the block's additional data.
//!
//! The provider that runs inside the PVF (`validate_block`) is *not* here and is *not*
//! [`VerifyingAdditionalDataProvider`]: it is the separate no_std `AdditionalDataReader`, coupled to
//! `cumulus-pallet-parachain-system`'s `validate_block` trie machinery, and lives there.
//!
//! [`AdditionalDataReader`]: https://docs.rs/cumulus-pallet-parachain-system

mod recorder;
mod verifier;

pub use recorder::RecordingAdditionalDataProvider;
pub use verifier::VerifyingAdditionalDataProvider;
