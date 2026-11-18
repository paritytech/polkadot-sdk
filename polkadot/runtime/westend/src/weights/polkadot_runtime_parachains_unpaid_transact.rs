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

//! Weights for `polkadot_runtime_parachains::unpaid_transact`
//!
//! NOT MEASURED: written by hand, to be replaced by running the benchmarks.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::Weight};
use core::marker::PhantomData;

/// Weight functions for `polkadot_runtime_parachains::unpaid_transact`.
pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> polkadot_runtime_parachains::unpaid_transact::WeightInfo for WeightInfo<T> {
	/// Storage: `UnpaidTransact::Windows` (r:0 w:128)
	/// Proof: `UnpaidTransact::Windows` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `p` is `[1, 128]`.
	fn remove_offboarded(p: u32, ) -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(Weight::from_parts(0, 0))
			.saturating_add(Weight::from_parts(2_000_000, 0).saturating_mul(p.into()))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(p.into())))
	}
	/// Storage: `UnpaidTransact::Windows` (r:128 w:0)
	/// Proof: `UnpaidTransact::Windows` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Paras::ParaLifecycles` (r:128 w:0)
	/// Proof: `Paras::ParaLifecycles` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `p` is `[1, 128]`.
	fn authorize_remove_offboarded(p: u32, ) -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1500))
			.saturating_add(Weight::from_parts(8_000_000, 0).saturating_mul(p.into()))
			.saturating_add(T::DbWeight::get().reads((2_u64).saturating_mul(p.into())))
			.saturating_add(Weight::from_parts(0, 5100).saturating_mul(p.into()))
	}
}
