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

//! Put implementations of functions from staging APIs here.
use crate::{configuration, initializer};

/// Implementation for `validation_code_bomb_limit` function from the runtime API
pub fn validation_code_bomb_limit<T: initializer::Config>() -> u32 {
	configuration::ActiveConfig::<T>::get().max_code_size *
		configuration::MAX_VALIDATION_CODE_COMPRESSION_RATIO
}
