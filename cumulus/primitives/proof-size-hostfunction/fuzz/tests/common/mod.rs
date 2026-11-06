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

//! Environment handling shared by `smoke.rs` and `v1_compat.rs`.

use cumulus_proof_size_hostfunction_fuzz::{script_from_seed, Scale};

pub fn env_u64(name: &str, default: u64) -> u64 {
	std::env::var(name)
		.ok()
		.and_then(|v| match v.strip_prefix("0x") {
			Some(hex) => u64::from_str_radix(hex, 16).ok(),
			None => v.parse().ok(),
		})
		.unwrap_or(default)
}

pub fn env_scale() -> Scale {
	match std::env::var("POV_FUZZ_SCALE").as_deref() {
		Ok("medium") => Scale::Medium,
		Ok("large") => Scale::Large,
		Ok("small") | Err(_) => Scale::Small,
		Ok(other) => panic!("POV_FUZZ_SCALE must be small, medium or large, not {other:?}"),
	}
}

/// A generated script with the environment's restrictions applied.
pub fn generate(seed: u64, max_scale: Scale) -> cumulus_proof_size_hostfunction_fuzz::Script {
	let mut script = script_from_seed(seed, max_scale);
	if std::env::var("POV_FUZZ_ROOT_LAST").is_ok_and(|v| v == "1") {
		script.drop_mid_block_roots();
	}
	script
}
