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

//! Bucketing for reputation changes.
//!
//! Tests assert on the *bucket* a reputation change falls into, not its exact magnitude. Mapping
//! is derived from `UnifiedReputationChange` variants: `Malicious(_) -> Malicious`,
//! benefits -> `Benefit`, all `CostMinor`/`CostMajor` flavours -> `Performance`.

use polkadot_node_network_protocol::{ReputationChange, UnifiedReputationChange};

/// Coarse classification of a reputation change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepBucket {
	/// A protocol-defined malicious behavior. `i32::MIN` magnitude.
	Malicious,
	/// A non-malicious cost (slow response, oversized message, ...).
	Performance,
	/// Good behavior reward.
	Benefit,
}

impl RepBucket {
	/// Map a [`UnifiedReputationChange`] (the typed reputation enum used at most call sites in
	/// the subsystem) to a coarse bucket.
	pub const fn from_unified(rep: &UnifiedReputationChange) -> Self {
		match rep {
			UnifiedReputationChange::Malicious(_) => RepBucket::Malicious,
			UnifiedReputationChange::CostMajor(_) |
			UnifiedReputationChange::CostMinor(_) |
			UnifiedReputationChange::CostMajorRepeated(_) |
			UnifiedReputationChange::CostMinorRepeated(_) => RepBucket::Performance,
			UnifiedReputationChange::BenefitMajor(_) |
			UnifiedReputationChange::BenefitMinor(_) |
			UnifiedReputationChange::BenefitMajorFirst(_) |
			UnifiedReputationChange::BenefitMinorFirst(_) => RepBucket::Benefit,
		}
	}

	/// Map a raw [`ReputationChange`] (i32 magnitude) into a bucket. Mirrors the magnitudes
	/// produced by `UnifiedReputationChange::cost_or_benefit`.
	pub fn from_raw(rep: &ReputationChange) -> Self {
		let v = rep.value;
		if v == i32::MIN {
			RepBucket::Malicious
		} else if v < 0 {
			RepBucket::Performance
		} else {
			RepBucket::Benefit
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn unified_mapping() {
		assert_eq!(
			RepBucket::from_unified(&UnifiedReputationChange::Malicious("bad")),
			RepBucket::Malicious
		);
		assert_eq!(
			RepBucket::from_unified(&UnifiedReputationChange::CostMinor("slow")),
			RepBucket::Performance
		);
		assert_eq!(
			RepBucket::from_unified(&UnifiedReputationChange::BenefitMajor("nice")),
			RepBucket::Benefit
		);
	}

	#[test]
	fn raw_mapping() {
		assert_eq!(RepBucket::from_raw(&ReputationChange::new(i32::MIN, "bad")), RepBucket::Malicious);
		assert_eq!(RepBucket::from_raw(&ReputationChange::new(-100, "slow")), RepBucket::Performance);
		assert_eq!(RepBucket::from_raw(&ReputationChange::new(100, "nice")), RepBucket::Benefit);
	}
}
