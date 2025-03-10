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
use super::Precompile;
use crate::{exec::ExecResult, Config, ExecReturnValue, GasMeter, RuntimeCosts};
use hex_literal::hex;
use pallet_revive_uapi::ReturnFlags;
use sp_core::H160;
pub const ECRECOVER: H160 = H160(hex!("0000000000000000000000000000000000000001"));

/// The ecrecover precompile.
pub struct ECRecover;

impl<T: Config> Precompile<T> for ECRecover {
	fn execute(gas_meter: &mut GasMeter<T>, i: &[u8]) -> ExecResult {
		gas_meter.charge(RuntimeCosts::EcdsaRecovery)?;

		let mut input = [0u8; 128];
		let len = i.len().min(128);
		input[..len].copy_from_slice(&i[..len]);

		let mut msg = [0u8; 32];
		let mut sig = [0u8; 65];

		msg[0..32].copy_from_slice(&input[0..32]);
		sig[0..32].copy_from_slice(&input[64..96]); // r
		sig[32..64].copy_from_slice(&input[96..128]); // s
		sig[64] = input[63]; // v

		// v can only be 27 or 28 on the full 32 bytes value.
		// https://github.com/ethereum/go-ethereum/blob/a907d7e81aaeea15d80b2d3209ad8e08e3bf49e0/core/vm/contracts.go#L177
		if input[32..63] != [0u8; 31] || ![27, 28].contains(&input[63]) {
			return Ok(ExecReturnValue { data: [0u8; 0].to_vec(), flags: ReturnFlags::empty() });
		}

		let data = match sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg) {
			Ok(pubkey) => {
				let mut address = sp_io::hashing::keccak_256(&pubkey);
				address[0..12].copy_from_slice(&[0u8; 12]);
				address.to_vec()
			},
			Err(_) => [0u8; 0].to_vec(),
		};

		Ok(ExecReturnValue { data, flags: ReturnFlags::empty() })
	}
}
