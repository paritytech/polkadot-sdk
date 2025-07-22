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

//! The pallet-revive Solidity fixtures contract implementation.

use alloy_core::hex::decode;

alloy_core::sol!("contracts/Playground.sol");
pub fn playground_bin() -> Vec<u8> {
	decode(include_str!("../contracts/build/Playground.bin")).unwrap()
}

alloy_core::sol!("contracts/Crypto.sol");
pub fn crypto_bin() -> Vec<u8> {
	decode(include_str!("../contracts/build/TestSha3.bin")).unwrap()
}

alloy_core::sol!("contracts/AddressPredictor.sol");
pub fn address_predictor_bin() -> Vec<u8> {
	decode(include_str!("../contracts/build/AddressPredictor.bin")).unwrap()
}
pub fn predicted_bin() -> Vec<u8> {
	decode(include_str!("../contracts/build/Predicted.bin")).unwrap()
}
pub fn predicted_bin_runtime() -> Vec<u8> {
	decode(include_str!("../contracts/build/Predicted.bin-runtime")).unwrap()
}

alloy_core::sol!("contracts/Flipper.sol");
pub fn flipper_bin() -> Vec<u8> {
	decode(include_str!("../contracts/build/Flipper.bin")).unwrap()
}
