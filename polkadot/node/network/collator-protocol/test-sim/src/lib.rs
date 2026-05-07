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
//!
//! # Public-API-only consumption of the subsystem under test
//!
//! This crate consumes `polkadot-collator-protocol` exclusively through its public API. **Do
//! not** reach for private types via `pub`-export hacks or `#[cfg(test)]`-gated escape hatches
//! in the subsystem crate.
//!
//! When a scenario or builder genuinely needs information that is currently private:
//!
//! - **Prefer** extending the contract enums (`contract::Effect` / `contract::Query`) so the
//!   need becomes an explicit observable, not implicit state coupling.
//! - **Or** drive the property via a public-API stimulus and assert on the resulting effect.
//! - **Last resort:** propose a small public-API addition to `polkadot-collator-protocol` with
//!   a justification in the PR description; reviewers can challenge it on the principle that
//!   tests reaching into internals defeat the framework's reason to exist.
//!
//! See the corresponding rule in `polkadot-collator-protocol`'s crate-level docs.

#![deny(missing_docs)]
#![deny(unused_crate_dependencies)]

pub mod builders;
pub mod contract;
pub mod harness;
pub mod impls;
pub mod report;
pub mod responder;
pub mod runtime;
