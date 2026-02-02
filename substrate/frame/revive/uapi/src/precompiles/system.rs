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

//! Information around the `System` pre-compile.

/// Address for the System pre-compile.
pub const SYSTEM_PRECOMPILE_ADDR: [u8; 20] =
	hex_literal::hex!("0000000000000000000000000000000000000900");

#[cfg(feature = "precompiles-sol-interfaces")]
alloy_core::sol!("sol/ISystem.sol");
