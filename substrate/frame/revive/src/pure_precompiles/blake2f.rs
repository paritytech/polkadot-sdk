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
use crate::{Config, ExecReturnValue, GasMeter, RuntimeCosts};
use pallet_revive_uapi::ReturnFlags;

mod eip_152;

/// The Blake2F precompile.
pub struct Blake2F;

impl<T: Config> Precompile<T> for Blake2F {
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> Result<ExecReturnValue, &'static str> {
		const BLAKE2_F_ARG_LEN: usize = 213;

		if input.len() != BLAKE2_F_ARG_LEN {
			return Err("invalid input length");
		}

		let mut rounds_buf: [u8; 4] = [0; 4];
		rounds_buf.copy_from_slice(&input[0..4]);
		let rounds: u32 = u32::from_be_bytes(rounds_buf);

		gas_meter.charge(RuntimeCosts::Blake2F(rounds))?;

		// we use from_le_bytes below to effectively swap byte order to LE if architecture is BE

		let mut h_buf: [u8; 64] = [0; 64];
		h_buf.copy_from_slice(&input[4..68]);
		let mut h = [0u64; 8];
		let mut ctr = 0;
		for state_word in &mut h {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&h_buf[(ctr * 8)..(ctr + 1) * 8]);
			*state_word = u64::from_le_bytes(temp);
			ctr += 1;
		}

		let mut m_buf: [u8; 128] = [0; 128];
		m_buf.copy_from_slice(&input[68..196]);
		let mut m = [0u64; 16];
		ctr = 0;
		for msg_word in &mut m {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&m_buf[(ctr * 8)..(ctr + 1) * 8]);
			*msg_word = u64::from_le_bytes(temp);
			ctr += 1;
		}

		let mut t_0_buf: [u8; 8] = [0; 8];
		t_0_buf.copy_from_slice(&input[196..204]);
		let t_0 = u64::from_le_bytes(t_0_buf);

		let mut t_1_buf: [u8; 8] = [0; 8];
		t_1_buf.copy_from_slice(&input[204..212]);
		let t_1 = u64::from_le_bytes(t_1_buf);

		let f = if input[212] == 1 {
			true
		} else if input[212] == 0 {
			false
		} else {
			return Err("invalid final flag");
		};

		eip_152::compress(&mut h, m, [t_0, t_1], f, rounds as usize);

		let mut output_buf = [0u8; u64::BITS as usize];
		for (i, state_word) in h.iter().enumerate() {
			output_buf[i * 8..(i + 1) * 8].copy_from_slice(&state_word.to_le_bytes());
		}

		Ok(ExecReturnValue { data: output_buf.to_vec(), flags: ReturnFlags::empty() })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::pure_precompiles::test::*;

	#[test]
	fn test_blake2f() -> Result<(), String> {
		test_precompile_test_vectors::<Blake2F>(include_str!("./testdata/9-blake2f.json"))?;
		test_precompile_failure_test_vectors::<Blake2F>(include_str!(
			"./testdata/9-blake2f-failures.json"
		))?;
		Ok(())
	}
}
