// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Provides the [`VALIDATION_WORKER`] for integration tests in Cumulus.
//!
//! The validation worker is used by the relay chain to validate parachains. This worker is placed
//! in an extra process to provide better security and to ensure that a worker can be killed etc.
//!
//! !!This should only be used for tests!!

pub use polkadot_node_core_pvf;

/// The path to the validation worker.
pub const VALIDATION_WORKER: &str = concat!(env!("OUT_DIR"), "/validation-worker");
