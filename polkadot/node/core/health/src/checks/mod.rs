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

//! The checks: one module per check, and the routing of the observations and the events to
//! them. A check holds its state, its reference value and its verdict, and knows nothing about
//! where its inputs come from.

mod reachability;

use crate::{Observation, LOG_TARGET};
use reachability::Reachability;

/// What the subsystem tells the checks.
pub enum Event {
	/// The tick interval elapsed.
	Tick,
}

/// The state of every check.
#[derive(Default)]
pub struct Checks {
	reachability: Reachability,
}

impl Checks {
	/// A producer reported an observation.
	/// Observations are routed by hand here to the relevant checks.
	pub fn on_observation(&mut self, observation: Observation) {
		gum::trace!(target: LOG_TARGET, ?observation, "Applying report");

		match observation {
			Observation::InboundConnection { peer } => {
				self.reachability.on_inbound_connection(peer)
			},
		}
	}

	/// The subsystem's clock moved on.
	pub fn on_event(&mut self, event: Event) {
		match event {
			Event::Tick => self.reachability.on_tick(),
		}
	}
}
