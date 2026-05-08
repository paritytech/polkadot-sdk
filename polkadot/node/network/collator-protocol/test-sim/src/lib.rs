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
//! Slim consumer of [`polkadot_subsystem_test_sim`]: re-exports the subsystem-agnostic core
//! and adds the collator-flavoured bits — `SubsystemUnderTest` adapters (`impls::*`), wire-
//! frame builder (`builders::peer`), the [`harness::CollatorSut`] convenience alias, and a
//! [`clock_adapter::ClockAdapter`] bridging the test-sim's `Clock` to
//! [`polkadot_collator_protocol::Clock`].
//!
//! Tests assert only on the *observable contract* (effects emitted to other subsystems /
//! wire). Internal queries (RuntimeApi, ProspectiveParachains, ChainApi, `CanSecond`) are
//! answered by a mock responder, never asserted.
//!
//! # Public-API-only consumption of the subsystem under test
//!
//! This crate consumes `polkadot-collator-protocol` exclusively through its public API. **Do
//! not** reach for private types via `pub`-export hacks or `#[cfg(test)]`-gated escape hatches
//! in the subsystem crate.
//!
//! When a scenario or builder genuinely needs information that is currently private:
//!
//! - **Prefer** extending the contract enums ([`contract::Effect`] / [`contract::Query`]) so
//!   the need becomes an explicit observable, not implicit state coupling.
//! - **Or** drive the property via a public-API stimulus and assert on the resulting effect.
//! - **Last resort:** propose a small public-API addition to `polkadot-collator-protocol`
//!   with a justification in the PR description.

#![deny(missing_docs)]
#![deny(unused_crate_dependencies)]

// Subsystem-agnostic core re-exported under the same paths the legacy crate exposed, so
// scenarios that import via `crate::aux`, `crate::chain`, etc. keep compiling unchanged.
pub use polkadot_subsystem_test_sim::{
	aux, chain, contract, report, responder, runtime, BoxedDelay, Clock,
};

pub mod builders;
pub mod clock_adapter;
pub mod harness;
pub mod impls;
pub mod scenarios;

/// Attribute macro for declaring deterministic-simulator tests. Expands to one `#[test]`
/// per registered subsystem-under-test implementation (currently `LegacyValidator` and
/// `ExperimentalValidator`); the same scenario body runs differentially against each and
/// any divergence in the observable contract fails the test for that impl.
pub use polkadot_collator_protocol_test_sim_macros::sim_test;
