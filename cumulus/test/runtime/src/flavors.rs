// Copyright (C) Parity Technologies (UK) Ltd.
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

pub mod spec_version_incremented {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_spec_version_incremented.rs"));
}

pub mod relay_parent_offset {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_relay_parent_offset.rs"));
}

pub mod elastic_scaling_500ms {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_elastic_scaling_500ms.rs"));
}

pub mod elastic_scaling {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_elastic_scaling.rs"));
}

pub mod elastic_scaling_12s_slot {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_elastic_scaling_12s_slot.rs"));
}

pub mod block_bundling {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_block_bundling.rs"));
}

pub mod sync_backing {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_sync_backing.rs"));
}

pub mod async_backing {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

pub mod async_backing_v3 {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_async_backing_v3.rs"));
}

pub mod async_backing_v3_rpo {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_async_backing_v3_rpo.rs"));
}

pub mod elastic_scaling_v3 {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_elastic_scaling_v3.rs"));
}

pub mod slot_duration_18s {
	#[cfg(feature = "std")]
	include!(concat!(env!("OUT_DIR"), "/wasm_binary_slot_duration_18s.rs"));
}

pub(crate) const SCHEDULING_V3_ENABLED: bool = cfg!(feature = "v3-descriptor");

pub(crate) const fn relay_parent_offset() -> u32 {
	if cfg!(feature = "relay-parent-offset-2") {
		return 2;
	}

	0
}

pub(crate) const fn slot_duration() -> u64 {
	if cfg!(feature = "18s-slot") {
		return 18000;
	}

	if cfg!(feature = "12s-slot") {
		return 12000;
	}

	6000
}

pub(crate) const fn block_processing_velocity() -> u32 {
	if cfg!(feature = "velocity-12") {
		return 12;
	}

	if cfg!(feature = "velocity-6") {
		return 6;
	}

	if cfg!(feature = "velocity-3") {
		return 3;
	}

	1
}

pub(crate) const fn unincluded_segment_capacity() -> u32 {
	if cfg!(feature = "sync-backing") {
		return 1;
	}

	// Without sync backing, the block flow is the following:
	//
	// - Collator produces the block(s) on relay chain block `X`
	// - In the meantime the relay chain is building block `X + 1`
	// - The collator sends the collation to the relay chain, and it gets backed on chain in relay
	//   block `X + 2`
	// - The collation then gets included on chain in relay block `X + 3`
	block_processing_velocity() * 3
}
