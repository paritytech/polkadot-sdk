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

//! Cumulus Collator implementation for Substrate.
use polkadot_node_subsystem::messages::CollatorProtocolMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::Id as ParaId;

pub mod collation;
pub mod metrics;
pub mod segment;
pub mod service;

/// Announce to the collator protocol that we are collating for `para_id`.
///
/// This must be done prior to distributing any segment, so that the collator protocol connects
/// to the validators backing our para.
pub async fn initialize_collator_subsystems(overseer_handle: &mut OverseerHandle, para_id: ParaId) {
	overseer_handle
		.send_msg(CollatorProtocolMessage::CollateOn(para_id), "StartCollator")
		.await;
}
