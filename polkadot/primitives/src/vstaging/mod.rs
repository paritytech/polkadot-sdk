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

//! Staging Primitives.

// Put any primitives used by staging APIs functions here

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

bitflags::bitflags! {
	#[derive(Default, TypeInfo, Encode, Decode, Serialize, Deserialize)]
	/// Bit indices in the `HostCoonfiguration.client_features` that correspond to different client features.
	pub struct ClientFeatures: u64 {
		/// Is availability chunk shuffling enabled.
		const AVAILABILITY_CHUNK_SHUFFLING = 0b1;
	}
}
