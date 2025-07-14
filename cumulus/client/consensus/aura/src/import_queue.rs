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

//! Parachain specific wrapper for the AuRa import queue.

use sc_consensus_aura::{AuraVerifier, CompatibilityMode};
use sc_telemetry::TelemetryHandle;
use std::sync::Arc;

/// Parameters of [`build_verifier`].
pub struct BuildVerifierParams<C, GetSlotFn> {
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// Something that can get the current slot.
	pub get_slot: GetSlotFn,
	/// The telemetry handle.
	pub telemetry: Option<TelemetryHandle>,
}

/// Build the [`AuraVerifier`].
pub fn build_verifier<P, C, GetSlotFn, N>(
	BuildVerifierParams { client, get_slot, telemetry }: BuildVerifierParams<C, GetSlotFn>,
) -> AuraVerifier<C, P, GetSlotFn, N> {
	sc_consensus_aura::build_verifier(sc_consensus_aura::BuildVerifierParams {
		client,
		get_slot,
		telemetry,
		compatibility_mode: CompatibilityMode::None,
	})
}
