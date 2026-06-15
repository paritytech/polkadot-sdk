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

use polkadot_core_primitives::Hash;
use sp_io::hashing::blake2_256;

use crate::{INNER_TAG, PEAK_TAG};

pub fn merge_inner(left: Hash, right: Hash) -> Hash {
    let mut preimage = Vec::with_capacity(1 + 32 + 32);
    preimage.push(INNER_TAG);
    preimage.extend_from_slice(left.as_bytes());
    preimage.extend_from_slice(right.as_bytes());
    blake2_256(&preimage).into()
}

pub fn merge_peaks(peaks: &[Hash]) -> Hash {
    let mut preimage = Vec::with_capacity(1 + 32 * peaks.len());
    preimage.push(PEAK_TAG);
    for peak in peaks {
        preimage.extend_from_slice(peak.as_bytes());
    }

    blake2_256(&preimage).into()
}

