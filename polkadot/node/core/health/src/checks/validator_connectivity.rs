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

//! Validator connectivity ratio: are we connected to enough validators of the current session.
//!
//! The input for the check is the authorities we hold a validation peer set connection with. At
//! every tick, the check passes when the share of the session's other validators among them is
//! above the threshold. It is not judged when we are not a validator of the session.

use crate::LOG_TARGET;
use polkadot_primitives::AuthorityDiscoveryId;
use std::collections::HashSet;

/// The share of the session's validators we must be connected to, in percent, to pass.
/// Equals gossip-support's `LOW_CONNECTIVITY_WARN_THRESHOLD` but still kept as a separate constant
/// in case we want to change the check behaviour.
const MIN_CONNECTED_PERCENT: usize = 85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
	/// `MIN_CONNECTED_PERCENT` requirement is satisfied.
	WellConnected,
	/// `MIN_CONNECTED_PERCENT` requirement is NOT satisfied.
	LowConnectivity,
}

/// Checks if we are connected to enough validators of the current session.
#[derive(Default)]
pub struct ValidatorConnectivity {
	/// The other validators of the current session. `None` when we are not one of them, or no
	/// session was seen yet.
	validators: Option<HashSet<AuthorityDiscoveryId>>,
	/// The latest snapshot of connected authorities. `None` when none arrived since the last tick.
	connected: Option<HashSet<AuthorityDiscoveryId>>,
	/// The result from the last check. `None` when it was not judged.
	outcome: Option<Outcome>,
}

impl ValidatorConnectivity {
	/// Note the other validators of the new session.
	pub fn on_new_session(&mut self, validators: Option<HashSet<AuthorityDiscoveryId>>) {
		if validators.is_none() {
			// Forget the outcome, so that the first check after we are a validator again logs.
			self.outcome = None;
		}
		self.validators = validators;
	}

	/// Note the latest snapshot of connected authorities.
	pub fn on_connected_validators(&mut self, authorities: HashSet<AuthorityDiscoveryId>) {
		self.connected = Some(authorities);
	}

	/// Determine the check outcome.
	pub fn on_tick(&mut self) {
		// Make sure there are some validators so that we can safely use it as a denominator.
		let Some(validators) = self.validators.as_ref().filter(|v| !v.is_empty()) else { return };
		// Each snapshot is judged once.
		let Some(snapshot) = self.connected.take() else {
			// No data => clear the outcome
			self.outcome = None;
			gum::warn!(
				target: LOG_TARGET,
				"No connected validators reported since the last check: validator connectivity was not judged",
			);
			return;
		};

		let total = validators.len();
		let connected = validators.intersection(&snapshot).count();
		let Some(percent) = connected.saturating_mul(100).checked_div(total) else { return };
		let outcome = if percent <= MIN_CONNECTED_PERCENT {
			Outcome::LowConnectivity
		} else {
			Outcome::WellConnected
		};
		gum::trace!(
			target: LOG_TARGET,
			?outcome,
			percent,
			connected,
			total,
			"Validator connectivity judged",
		);

		if self.outcome.replace(outcome) == Some(outcome) {
			return;
		}

		// Log only on state change
		match outcome {
			Outcome::WellConnected => gum::debug!(
				target: LOG_TARGET,
				connected,
				total,
				"Connected to enough validators of the session",
			),
			Outcome::LowConnectivity => gum::warn!(
				target: LOG_TARGET,
				connected,
				total,
				"Connected to {percent}% of the validators of the session, at most \
				 {MIN_CONNECTED_PERCENT}% means low connectivity. Check that the node publishes \
				 its addresses, the firewall and that peers are able to reach the node",
			),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_application_crypto::ByteArray;

	fn authorities(range: std::ops::Range<u8>) -> HashSet<AuthorityDiscoveryId> {
		range.map(|i| AuthorityDiscoveryId::from_slice(&[i; 32]).unwrap()).collect()
	}

	/// The outcome at a tick in a session of `total` validators with `connected` of them connected.
	fn outcome_with(connected: u8, total: u8) -> Option<Outcome> {
		let mut check = ValidatorConnectivity::default();
		check.on_new_session(Some(authorities(0..total)));
		check.on_connected_validators(authorities(0..connected));
		check.on_tick();
		check.outcome
	}

	#[test]
	fn connected_above_the_threshold() {
		assert_eq!(outcome_with(18, 20), Some(Outcome::WellConnected));
		assert_eq!(outcome_with(20, 20), Some(Outcome::WellConnected));
	}

	#[test]
	fn low_connectivity_at_the_threshold() {
		assert_eq!(outcome_with(17, 20), Some(Outcome::LowConnectivity));
	}

	#[test]
	fn low_connectivity_below_the_threshold() {
		assert_eq!(outcome_with(16, 20), Some(Outcome::LowConnectivity));
		assert_eq!(outcome_with(0, 20), Some(Outcome::LowConnectivity));
	}

	#[test]
	fn not_judged_without_validators() {
		let mut check = ValidatorConnectivity::default();
		check.on_connected_validators(authorities(0..20));
		check.on_tick();
		assert_eq!(check.outcome, None);

		check.on_new_session(Some(HashSet::new()));
		check.on_tick();
		assert_eq!(check.outcome, None);
	}

	#[test]
	fn not_judged_without_a_new_snapshot() {
		let mut check = ValidatorConnectivity::default();

		// New session, but no report
		check.on_new_session(Some(authorities(0..20)));
		check.on_tick();
		assert_eq!(check.outcome, None);

		// Once the report arrives there is an outcome
		check.on_connected_validators(authorities(0..20));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::WellConnected));

		// The snapshot is not judged twice
		check.on_tick();
		assert_eq!(check.outcome, None);

		// ... even after a new session
		check.on_connected_validators(authorities(0..20));
		check.on_tick();
		check.on_new_session(Some(authorities(20..40)));
		check.on_tick();
		assert_eq!(check.outcome, None);
	}

	#[test]
	fn the_outcome_follows_the_session_validators() {
		let mut check = ValidatorConnectivity::default();
		check.on_new_session(Some(authorities(0..20)));
		check.on_connected_validators(authorities(0..20));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::WellConnected));

		check.on_new_session(Some(authorities(20..40)));
		check.on_connected_validators(authorities(0..20));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::LowConnectivity));

		// The outcome is forgotten when we are not a validator.
		check.on_new_session(None);
		check.on_tick();
		assert_eq!(check.outcome, None);
	}

	#[test]
	fn authorities_outside_the_session_do_not_count() {
		let mut check = ValidatorConnectivity::default();
		check.on_new_session(Some(authorities(0..20)));
		check.on_connected_validators(authorities(10..40));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::LowConnectivity));
	}
}
