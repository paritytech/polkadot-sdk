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

//! Why `jam_validate_block` rejected a candidate for a provable lie against JAM data.

use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// A fault of a candidate that contradicts JAM data the validators checked, so the runtime can
/// slash its author for it.
///
/// `jam_validate_block` aborts the refine with the SCALE encoding of this value as the
/// `report_error` payload, which the Parachain Service records in the para's log as
/// `RefineLog::Opaque`. Malformed input is not a `RefineError`: it panics.
#[derive(Encode, Decode, Debug, PartialEq, Eq, TypeInfo)]
pub enum RefineError {
	/// A block of a package built at authoring time names JAM anchors other than the refine
	/// context's in its `JamParent` digest.
	#[codec(index = 0)]
	JamParentMismatch,
	/// A block of a resubmitted package names a JAM anchor or lookup anchor that the package's
	/// anchor chain does not prove at the claimed slot.
	#[codec(index = 1)]
	JamParentUnproven,
	/// The anchor chain of a resubmitted package does not lead from the refine context's anchor
	/// through linked JAM headers.
	#[codec(index = 2)]
	AnchorChainInvalid,
	/// The JAM state proof a block carries is rooted at a state root other than the one derived
	/// for its anchor.
	#[codec(index = 3)]
	JamProofRootMismatch,
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;

	/// The payload log readers decode (`cumulus/zombienet/zombienet-sdk/README.md`).
	#[test]
	fn payload_is_the_variant_index() {
		let table = [
			(RefineError::JamParentMismatch, 0u8),
			(RefineError::JamParentUnproven, 1),
			(RefineError::AnchorChainInvalid, 2),
			(RefineError::JamProofRootMismatch, 3),
		];
		for (error, index) in table {
			assert_eq!(error.encode(), vec![index]);
			assert_eq!(RefineError::decode(&mut &[index][..]), Ok(error));
		}
		assert!(RefineError::decode(&mut &[4u8][..]).is_err());
	}
}
