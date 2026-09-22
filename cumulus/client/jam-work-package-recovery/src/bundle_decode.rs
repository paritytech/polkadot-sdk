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
//! open with the SCALE-encoded work package. The first work item's `payload` holds the
//! SCALE-encoded `ParachainCandidate`; its `pov` field is a SCALE-encoded `ParachainBlockData`.

use codec::DecodeAll;
use cumulus_jam_interface::WorkPackageHash;
use cumulus_primitives_core::ParachainBlockData;
use jam_codec::Decode;
use jam_std_common::ImmutableWorkPackage;
use parachain_service_core::candidate::ParachainCandidate;
use sp_additional_data::AdditionalData;
use sp_runtime::traits::Block as BlockT;

/// Decode bundle bytes from [`crate::JamBundleRecovery::recover_bundle`] into parachain blocks.
///
/// The decode chain:
/// 1. `bytes` → `ImmutableWorkPackage::decode` (reads the work package from the bundle head)
/// 2. `package.items[0].payload.0` → SCALE-encoded `ParachainCandidate`
/// 3. `ParachainCandidate::decode` → `{ validation_code_hash, pov }`
/// 4. `ParachainBlockData::<Block>::decode_all(&pov)` → blocks + per-block additional data
///
/// Returns an error on any decode failure. Never panics on network bytes: an empty `items`
/// vector is handled as an explicit error rather than an index panic. A bundle for another
/// service (e.g. items present but index 0 holds a non-parachain payload) surfaces as a
/// `ParachainCandidate` decode failure and is also handled without panicking.
pub fn decode_bundle<Block: BlockT>(
	bytes: &[u8],
) -> Result<Vec<(Block, Option<AdditionalData>)>, String> {
	// Step 1: decode the ImmutableWorkPackage from the bundle head.
	// `Immutable<T>::Encode` writes its stored bytes inline (no outer length prefix), so the
	// bundle bytes start with the raw SCALE-encoded work package; anything after it (extrinsics,
	// imports, erasure proofs) is left unread — we only need the payload.
	let package = ImmutableWorkPackage::decode(&mut &bytes[..])
		.map_err(|e| format!("bundle: ImmutableWorkPackage decode: {e}"))?;

	// Step 2: guard against an empty items list.
	// A valid parachain work package always has exactly one item at index 0. Zero items means a
	// corrupt or non-parachain bundle; we return an error rather than panic on the index.
	let first_item = package
		.items
		.iter()
		.next()
		.ok_or_else(|| "bundle: work package carries no items".to_string())?;

	// Step 3: payload.0 is the SCALE-encoded `ParachainCandidate`.
	let payload_bytes: &[u8] = &first_item.payload.0;

	// Step 4: decode the candidate.
	let candidate = <ParachainCandidate as codec::Decode>::decode(&mut &payload_bytes[..])
		.map_err(|e| format!("bundle: ParachainCandidate decode: {e}"))?;

	// Step 5: decode ParachainBlockData from the PoV.
	// `decode_all` rejects trailing bytes so a partial-write from a buggy collator surfaces
	// immediately rather than silently producing incomplete block data.
	let block_data = ParachainBlockData::<Block>::decode_all(&mut &candidate.pov[..])
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
