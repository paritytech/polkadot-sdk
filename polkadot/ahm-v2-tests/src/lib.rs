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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! # AHM Phase 2 Behaviour Tests
//!
//! Documents the current relay chain behaviour of the subsystems that are candidates to move
//! off the relay chain in AHM phase 2 (parachain registration and lifecycle management),
//! by exercising them against the real Westend runtime.
//!
//! These tests serve as the behavioural baseline: once functionality moves (e.g. registrar
//! user interactions to the Coretime chain), equivalent scenarios must still hold from the
//! user's point of view.
//!
//! Everything runs on genesis-built state in plain `TestExternalities` — no snapshots, no
//! nodes. Sessions are advanced manually via the `parachains_shared` test helpers, since Babe
//! never rotates sessions without real block production (see `harness`).

#![cfg(test)]

mod harness;
mod registrar;
