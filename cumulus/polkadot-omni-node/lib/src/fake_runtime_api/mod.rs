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

//! In an ideal world this would be one runtime which would simplify the code massively.
//! This is not an ideal world - Polkadot Asset Hub has a different key type.

mod utils;

use utils::{impl_node_runtime_apis, imports::*};

#[allow(dead_code)]
type CustomBlock = crate::common::types::Block<u32>;

pub mod aura_sr25519 {
	use super::*;
	#[allow(dead_code)]
	struct FakeRuntime;
	impl_node_runtime_apis!(FakeRuntime, CustomBlock, sp_consensus_aura::sr25519::AuthorityId);
}

pub mod aura_ed25519 {
	use super::*;
	#[allow(dead_code)]
	struct FakeRuntime;
	impl_node_runtime_apis!(FakeRuntime, CustomBlock, sp_consensus_aura::ed25519::AuthorityId);
}
