// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Minimal Pallet that injects a ParachainId into Runtime storage from

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{decl_module, decl_storage, traits::Get};

use cumulus_primitives::ParaId;

/// Configuration trait of this pallet.
pub trait Trait: frame_system::Trait {}

impl <T: Trait> Get<ParaId> for Module<T> {
	fn get() -> ParaId {
		Self::parachain_id()
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as ParachainUpgrade {
		ParachainId get(fn parachain_id) config(): ParaId = 100.into();
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
}
