// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

use sp_externalities::ExternalitiesExt;

#[cfg(feature = "std")]
use sp_proof_size_ext::ProofSizeExt;
use sp_runtime_interface::runtime_interface;

#[runtime_interface]
pub trait PovReclaimHostFunctions {
	fn current_storage_proof_size(&mut self) -> u32 {
		match self.extension::<ProofSizeExt>() {
			Some(ext) => ext.current_storage_proof_size(),
			None => 0,
		}
	}
}
