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

use polkadot_node_subsystem::messages::PvfExecKind;

/// A priority assigned to preparation of a PVF.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
	/// Normal priority for things that do not require immediate response, but still need to be
	/// done pretty quick.
	///
	/// Backing falls into this category.
	Normal,
	/// This priority is used for requests that are required to be processed as soon as possible.
	///
	/// Disputes and approvals are on a critical path and require execution as soon as
	/// possible to not delay finality.
	Critical,
}

impl Priority {
	/// Returns `true` if `self` is `Critical`
	pub fn is_critical(self) -> bool {
		self == Priority::Critical
	}
}

impl From<PvfExecKind> for Priority {
	fn from(priority: PvfExecKind) -> Self {
		match priority {
			PvfExecKind::Dispute => Priority::Critical,
			PvfExecKind::Approval => Priority::Critical,
			PvfExecKind::BackingSystemParas => Priority::Normal,
			PvfExecKind::Backing => Priority::Normal,
		}
	}
}
