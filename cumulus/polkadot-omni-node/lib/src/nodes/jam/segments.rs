// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The export segment a parachain block's work package publishes, and the tree around it.
//!
//! Every package exports exactly one segment — segment 0, the block's header — so a child
//! package can import it and prove in-core which block it builds on. The layout is a byte
//! contract with the parachain service (parasim today): **the SCALE encoding of `Vec<u8>`
//! holding the encoded header** (compact length prefix ‖ header bytes), zero-padded to
//! `SEGMENT_LEN`. The service strips the prefix and rejects a zero-length one, which is what a
//! *failed* parent's zeroed export decodes as.
//!
//! The collator has to build the tree itself twice over: to hand a child's guarantors the parent
//! segment inline (they cannot fetch it from DA yet, see the bundle assembly in
//! [`super::collation_task`]), and to recognise an in-flight work report as belonging to a block
//! it holds by recomputing that report's `segroot` from the block's header.

use codec::Encode;
use jam_std_common::{CdMerkleProof, import_proofs};
use jam_types::{SEGMENT_LEN, SegmentTreeRoot};

/// A package's whole export: its single segment, the proof of that segment against the tree, and
/// the tree root that ends up in the work report's availability specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Export {
	/// Segment 0 — the length-prefixed, zero-padded header.
	pub segment: Box<[u8; SEGMENT_LEN]>,
	/// The import proof for index 0, which a child's bundle carries alongside the segment.
	pub proof: CdMerkleProof,
	/// The segment-tree root guarantors authenticate the import against.
	pub segroot: SegmentTreeRoot,
}

/// Lay an encoded header out as export segment 0.
pub(crate) fn export_segment(encoded_header: &[u8]) -> Result<Box<[u8; SEGMENT_LEN]>, String> {
	let mut segment = Vec::with_capacity(SEGMENT_LEN);
	encoded_header.encode_to(&mut segment);
	let encoded_len = segment.len();
	if encoded_len > SEGMENT_LEN {
		return Err(format!(
			"the encoded header does not fit a segment: {encoded_len} > {SEGMENT_LEN}"
		));
	}
	segment.resize(SEGMENT_LEN, 0);
	Ok(segment.into_boxed_slice().try_into().expect("resized to SEGMENT_LEN just above; qed"))
}

/// The export of a package that exports exactly this one header.
pub(crate) fn export_of(encoded_header: &[u8]) -> Result<Export, String> {
	let segment = export_segment(encoded_header)?;
	let (mut proofs, segroot) = import_proofs(std::slice::from_ref(&segment));
	if proofs.len() != 1 {
		return Err(format!("one segment yielded {} import proofs", proofs.len()));
	}
	Ok(Export { segment, proof: proofs.remove(0), segroot })
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Decode;
	use cumulus_test_runtime::Header as TestHeader;
	use sp_core::H256;
	use sp_runtime::traits::Header as HeaderT;

	fn test_header(number: u32) -> TestHeader {
		TestHeader::new(
			number,
			H256::repeat_byte(1),
			H256::repeat_byte(2),
			H256::repeat_byte(3),
			Default::default(),
		)
	}

	/// The byte contract with the parachain service: whatever we pad into a segment, the reader
	/// that runs in-core hands back unchanged. Checked against the service's own reader so a
	/// change on either side fails here rather than on a live network.
	#[test]
	fn the_exported_segment_is_read_back_by_the_parachain_service() {
		let encoded_header = test_header(5).encode();
		let segment = export_segment(&encoded_header).expect("a header fits a segment");

		assert_eq!(segment.len(), SEGMENT_LEN);
		assert_eq!(
			parasim_service::imported_header(&segment[..]),
			Ok(&encoded_header[..]),
			"the service reads back exactly the header we exported",
		);
	}

	/// A zeroed segment — what JAM commits for an item whose refine *failed* — decodes as an
	/// empty vector, whose hash is a constant anyone could name as their parent. This is the
	/// case the service's guard exists for; pinned here so the padding scheme keeps producing
	/// something that guard can tell apart from a real header.
	#[test]
	fn a_zero_segment_decodes_as_an_empty_header_and_is_rejected() {
		let zeroed = [0u8; SEGMENT_LEN];

		assert_eq!(Vec::<u8>::decode(&mut &zeroed[..]), Ok(Vec::new()));
		assert!(parasim_service::imported_header(&zeroed).is_err());
	}

	/// The bound is real: nothing bigger than a segment can be exported, and a header that grew
	/// past it has to surface as an error rather than a truncated export.
	#[test]
	fn an_over_long_header_does_not_fit_a_segment() {
		assert!(export_segment(&vec![7u8; SEGMENT_LEN]).is_err());
	}

	/// 5.4 recognises another collator's in-flight report by recomputing this root from the
	/// header of a block it holds, so the same header must always give the same root, and two
	/// different headers must not collide.
	#[test]
	fn the_segment_root_is_a_deterministic_function_of_the_header() {
		let one = export_of(&test_header(5).encode()).expect("a header fits a segment");
		let same = export_of(&test_header(5).encode()).expect("a header fits a segment");
		let other = export_of(&test_header(6).encode()).expect("a header fits a segment");

		assert_eq!(one, same);
		assert_ne!(one.segroot, other.segroot);
	}
}
