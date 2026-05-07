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

//! The harness layer: Sim struct, observation recorder, query dispatcher.
//!
//! These are subsystem-agnostic in spirit (parameterized over the `contract::Effect` /
//! `contract::Query` enums) but the first iteration is wired specifically for the
//! collator-protocol. Generalizing to a `SubsystemContract` trait happens once a second subsystem
//! is onboarded (see plan).

pub mod dispatcher;
pub mod observation;
pub mod recorder;

pub use dispatcher::Dispatcher;
pub use observation::{Observation, Stamped};
pub use recorder::Recorder;
