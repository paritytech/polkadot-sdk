// Copyright Parity Technologies (UK) Ltd.
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

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::Weight};
use core::marker::PhantomData;

/// Weight functions for `cumulus_pallet_xcmp_queue`.
pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> cumulus_pallet_parachain_system::WeightInfo for WeightInfo<T> {
	/// Storage: ParachainSystem LastDmqMqcHead (r:1 w:1)
	/// Proof Skipped: ParachainSystem LastDmqMqcHead (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: ParachainSystem ReservedDmpWeightOverride (r:1 w:0)
	/// Proof Skipped: ParachainSystem ReservedDmpWeightOverride (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: MessageQueue BookStateFor (r:1 w:1)
	/// Proof: MessageQueue BookStateFor (max_values: None, max_size: Some(52), added: 2527, mode: MaxEncodedLen)
	/// Storage: MessageQueue ServiceHead (r:1 w:1)
	/// Proof: MessageQueue ServiceHead (max_values: Some(1), max_size: Some(5), added: 500, mode: MaxEncodedLen)
	/// Storage: ParachainSystem ProcessedDownwardMessages (r:0 w:1)
	/// Proof Skipped: ParachainSystem ProcessedDownwardMessages (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: MessageQueue Pages (r:0 w:16)
	/// Proof: MessageQueue Pages (max_values: None, max_size: Some(65585), added: 68060, mode: MaxEncodedLen)
	/// The range of component `n` is `[0, 1000]`.
	fn enqueue_inbound_downward_messages(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `12`
		//  Estimated: `8013`
		// Minimum execution time: 1_645_000 picoseconds.
		Weight::from_parts(1_717_000, 0)
			.saturating_add(Weight::from_parts(0, 8013))
			// Standard Error: 12_258
			.saturating_add(Weight::from_parts(24_890_934, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(4))
	}
}