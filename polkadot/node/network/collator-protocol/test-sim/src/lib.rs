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
//!
//! # Why a hand-rolled harness instead of a real `polkadot-overseer`?
//!
//! Considered and rejected. Two load-bearing reasons:
//!
//! 1. **`tokio::time::pause()` does not control `futures_timer::Delay`**, and orchestra's
//!    `TimeoutExt::timeout` plus the overseer's metrics metronome both use
//!    `futures_timer::Delay`. Running the harness on a paused tokio runtime would leave
//!    multiple time sources still ticking against real wall-clock — kills determinism.
//! 2. **Precise quiescence.** `LocalPool::run_until_stalled()` polls every spawned task
//!    until each returns `Pending`. Tokio current_thread has no equivalent; the folklore
//!    is `yield_now` loops or sleeps, both heuristic. Scenario tests that read like specs
//!    require deterministic ordering, not best-effort settling.
//!
//! Subsystem-bench's mock subsystems are real `overseer::Subsystem` impls designed for a
//! multi-thread tokio runtime against a benchmark workload — different problem, not
//! reusable. Malus's `MessageInterceptor` wraps a real subsystem inside a real overseer
//! driven by `polkadot_cli::run_node` — wrong layer entirely.
//!
//! When extending the model (new query variants, new stub behaviours) those crates'
//! pattern-match arms are useful as a checklist, but the code shape stays here.

#![deny(missing_docs)]
#![deny(unused_crate_dependencies)]

pub mod aux;
pub mod builders;
pub mod chain;
pub mod contract;
pub mod harness;
pub mod impls;
pub mod report;
pub mod responder;
pub mod runtime;
pub mod scenarios;

/// Attribute macro for declaring deterministic-simulator tests. Expands to one `#[test]`
/// per registered subsystem-under-test implementation (currently `LegacyValidator` and
/// `ExperimentalValidator`); the same scenario body runs differentially against each and
/// any divergence in the observable contract fails the test for that impl.
pub use polkadot_collator_protocol_test_sim_macros::sim_test;
