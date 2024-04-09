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

//! Trait for recording XCMs and a dummy implementation.

use xcm::latest::Xcm;

/// Trait for recording XCMs.
pub trait RecordXcm {
	/// Whether or not we should record incoming XCMs.
	fn should_record() -> bool;
	/// Record `xcm`.
	fn record(xcm: Xcm<()>);
}

impl RecordXcm for () {
	fn should_record() -> bool {
		false
	}

	fn record(_: Xcm<()>) {}
}
