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

//! Weights for `pallet-registrar-relay`.

use frame_support::weights::Weight;

/// Weight functions needed for `pallet-registrar-relay`.
pub trait WeightInfo {
	/// `h` is the length of the genesis head data in bytes.
	fn authorize_code(h: u32) -> Weight;
	/// `c` is the length of the validation code in bytes.
	///
	/// This call carries no signature and pays no fee, so its weight is the only thing bounding
	/// how much of a block it can take up.
	fn apply_authorized_code(c: u32) -> Weight;
	/// Cost of deciding whether an unsigned `apply_authorized_code` is acceptable.
	///
	/// `c` is the length of the validation code in bytes; the check hashes the whole blob.
	fn authorize_apply_authorized_code(c: u32) -> Weight;
	fn cancel_authorization() -> Weight;
}

/// Zero weights, for tests and mocks only.
impl WeightInfo for () {
	fn authorize_code(_h: u32) -> Weight {
		Weight::zero()
	}
	fn apply_authorized_code(_c: u32) -> Weight {
		Weight::zero()
	}
	fn authorize_apply_authorized_code(_c: u32) -> Weight {
		Weight::zero()
	}
	fn cancel_authorization() -> Weight {
		Weight::zero()
	}
}
