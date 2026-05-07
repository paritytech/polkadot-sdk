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

//! Real subsystems wired into the harness as auxiliary slots.
//!
//! Each module spawns a production subsystem on the [`Sim`]'s `Executor` and exposes a
//! `register_*` helper that returns a `(Slot, outbound_rx)` pair to hand to
//! [`Sim::register_aux`].
//!
//! [`Sim`]: crate::harness::Sim
//! [`Sim::register_aux`]: crate::harness::Sim::register_aux

pub mod availability_store;
pub mod backing;
pub mod can_second_stub;
pub mod candidate_validation;
pub mod noop;
pub mod prospective;

pub use availability_store::AvailabilityStoreStub;
pub use backing::CandidateBackingAux;
pub use can_second_stub::CanSecondStub;
pub use candidate_validation::{CandidateOutputs, CandidateValidationStub, Verdict};
pub use noop::{AvailabilityDistributionNoop, ProvisionerNoop, StatementDistributionNoop};
pub use prospective::ProspectiveParachainsAux;
