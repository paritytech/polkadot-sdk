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

use sp_runtime::traits::Block as BlockT;
use substrate_test_runtime_client::runtime::Block;

struct Runtime {}

sp_api::decl_runtime_apis! {
	#[api_version(2)]
	pub trait Api {
		fn test1();
		fn test2();
		#[api_version(3)]
		fn test3();
	}
}

sp_api::impl_runtime_apis! {
	#[api_version(4)]
	impl self::Api<Block> for Runtime {
		fn test1() {}
		fn test2() {}
		fn test3() {}
	}

	impl sp_api::Core<Block> for Runtime {
		fn version() -> sp_version::RuntimeVersion {
			unimplemented!()
		}
		fn execute_block(_: Block) {
			unimplemented!()
		}
		fn initialize_block(_: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
			unimplemented!()
		}
	}
}

fn main() {}
