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

//! Deterministic simulator-based test framework for the Polkadot collator-protocol subsystem.
//!
//! Tests written against this framework assert only on the *observable contract* (effects emitted
//! to other subsystems / wire). Internal queries (RuntimeApi, ProspectiveParachains, ChainApi,
//! `CanSecond`) are answered by a mock responder, never asserted.
//!
//! Internal module layering mirrors the eventual crate-level boundaries when the framework is
//! generalized to a second subsystem. See `runtime/`, `harness/`, `responder/`, `contract/`,
//! `builders/`, `impls/`, `report/`.

#![deny(missing_docs)]
#![deny(unused_crate_dependencies)]

pub mod contract;
pub mod harness;
pub mod report;
pub mod responder;
pub mod runtime;
