// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Bundle decode chain: wire bytes → parachain blocks.
//!
//! The wire format is a JAM bundle (`ImmutableBundle = Immutable<Bundle>`). `Bundle::Decode` reads
//! the `ImmutableWorkPackage` first — inline, with no outer length prefix — so the bundle bytes
//! open with the SCALE-encoded work package, immediately followed by the concatenated extrinsic
//! data. The first work item's first extrinsic holds the SCALE-encoded `ParachainBlockData`, and
//! its `ExtrinsicSpec` carries the length and hash that bind those bytes to the package.

use codec::DecodeAll;
use cumulus_jam_interface::WorkPackageHash;
use cumulus_primitives_core::ParachainBlockData;
use jam_codec::Decode;
use jam_std_common::{hash_raw, ImmutableWorkPackage};
use sp_additional_data::AdditionalData;
use sp_runtime::traits::Block as BlockT;

/// Decode bundle bytes from [`crate::JamBundleRecovery::recover_bundle`] into parachain blocks.
///
/// The decode chain:
/// 1. `bytes` → `ImmutableWorkPackage::decode` (reads the work package from the bundle head)
/// 2. the bytes straight after the package → the work item's extrinsic data
/// 3. `items[0].extrinsics[0]` → the spec whose `len` slices that data and whose `hash` must equal
///    `jam_std_common::hash_raw` of the sliced bytes
/// 4. `ParachainBlockData::<Block>::decode_all(extrinsic data)` → blocks + per-block additional
///    data
///
/// Returns an error on any decode failure. Never panics on network bytes: an empty `items` vector
/// or an item with no extrinsics is handled as an explicit error rather than an index panic, and a
/// truncated bundle or a hash that does not match its spec is rejected too.
pub fn decode_bundle<Block: BlockT>(
	bytes: &[u8],
) -> Result<Vec<(Block, Option<AdditionalData>)>, String> {
	// Step 1: decode the ImmutableWorkPackage from the bundle head.
	// `Immutable<T>::Encode` writes its stored bytes inline (no outer length prefix), so the
	// bundle bytes start with the raw SCALE-encoded work package. Decoding advances `input` past
	// it, leaving the extrinsic data (and, after that, import segments and proofs) unread.
	let mut input = bytes;
	let package = ImmutableWorkPackage::decode(&mut input)
		.map_err(|e| format!("bundle: ImmutableWorkPackage decode: {e}"))?;

	// Step 2: the first item's first extrinsic carries the PoV.
	let first_item = package
		.items
		.iter()
		.next()
		.ok_or_else(|| "bundle: work package carries no items".to_string())?;
	let spec = first_item
		.extrinsics
		.iter()
		.next()
		.ok_or_else(|| "bundle: work item carries no extrinsics".to_string())?;

	// Step 3: the spec's length slices the extrinsic data that follows the package, and its hash
	// must match. A short bundle or a forged length is an error, not a panic.
	let len = spec.len as usize;
	if input.len() < len {
		return Err(format!(
			"bundle: extrinsic data truncated: spec wants {len} bytes, bundle has {}",
			input.len()
		));
	}
	let (extrinsic_data, _) = input.split_at(len);
	if hash_raw(extrinsic_data) != spec.hash.0 {
		return Err("bundle: extrinsic data does not match its spec hash".to_string());
	}

	// Step 4: decode ParachainBlockData from the extrinsic bytes.
	// `decode_all` rejects trailing bytes so a partial-write from a buggy collator surfaces
	// immediately rather than silently producing incomplete block data.
	let block_data = ParachainBlockData::<Block>::decode_all(&mut &extrinsic_data[..])
		.map_err(|e| format!("bundle: ParachainBlockData decode: {e}"))?;

	Ok(block_data.into_blocks_and_additional_data())
}

/// The authentic work-package hash of a recovered bundle.
///
/// A bundle opens with the SCALE-encoded work package, and `ImmutableWorkPackage` records
/// exactly those bytes; its hash is blake2b-256 over them — the same function the author used
/// (`collation_task::work_package_hash`). Signing is non-deterministic, so this hash cannot be
/// recomputed from a digest; a recovered bundle carries the author's own bytes and therefore
/// the author's own hash.
pub fn bundle_work_package_hash(bytes: &[u8]) -> Result<WorkPackageHash, String> {
	let package = ImmutableWorkPackage::decode(&mut &bytes[..])
		.map_err(|e| format!("bundle: ImmutableWorkPackage decode: {e}"))?;
	Ok(package.hash())
}
