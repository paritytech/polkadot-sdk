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

//! Deterministic simulator-based test framework for the Polkadot
//! prospective-parachains subsystem.
//!
//! Slim consumer of [`polkadot_subsystem_test_sim`]. Provides a
//! [`ProspectiveParachains`] `SubsystemUnderTest` adapter so scenarios can drive the real
//! production subsystem inside the deterministic harness.
//!
//! Prospective-parachains has no `Clock` injection point — its time-dependent behaviour
//! flows through `OverseerSignal::ActiveLeaves` (which carries a `BlockNumber` per leaf)
//! plus the chain model's `RuntimeApi` / `ChainApi` responses. No `ClockAdapter` is needed.
//!
//! # Test shape
//!
//! Unlike collator-protocol, prospective-parachains' observable contract is dominated by
//! query-response: scenarios send a typed message (e.g.
//! `ProspectiveParachainsMessage::GetBackableCandidates`) and assert on the value the
//! subsystem writes back to the embedded `oneshot::Sender`. The harness's `Sim::expect` /
//! `expect_no` family targets recorded `Effect`s — for prospective most assertions are
//! direct on the oneshot reply path. See [`scenarios`] for examples.
//!
//! Outbound messages prospective *does* emit — RuntimeApi / ChainApi queries — flow into
//! the harness's responder chain (mostly answered by [`polkadot_subsystem_test_sim::chain::ChainModel`]),
//! never into [`Effect`] records.
//!
//! [`Effect`]: polkadot_subsystem_test_sim::contract::Effect

#![deny(missing_docs)]
#![deny(unused_crate_dependencies)]

pub mod impls;
pub mod world;

pub use impls::ProspectiveParachains;
