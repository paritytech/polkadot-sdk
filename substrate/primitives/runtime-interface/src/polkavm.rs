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

pub use polkavm_derive::{polkavm_export, polkavm_import};

#[polkavm_derive::polkavm_define_abi(allow_extra_input_registers)]
pub mod polkavm_abi {}

impl self::polkavm_abi::FromHost for *mut u8 {
	type Regs = (u32,);

	#[inline]
	fn from_host((value,): Self::Regs) -> Self {
		value as *mut u8
	}
}
