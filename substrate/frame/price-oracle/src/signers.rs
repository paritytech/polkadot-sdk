// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! The set of keys whose price reports are accepted.

use alloc::vec::Vec;

/// Supplies the keys whose reports the pallet accepts.
pub trait Signers<Id> {
	/// All keys accepted right now.
	///
	/// Around a change of the signer set this includes both the outgoing and the incoming set,
	/// so that reports signed shortly before the change remain valid.
	fn signers() -> Vec<Id>;
}
