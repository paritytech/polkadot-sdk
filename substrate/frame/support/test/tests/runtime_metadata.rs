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
#![allow(useless_deprecated)]

use frame_support::{derive_impl, traits::ConstU32};
use scale_info::{form::MetaForm, meta_type};
use sp_metadata_ir::{
	DeprecationStatusIR, RuntimeApiMetadataIR, RuntimeApiMethodMetadataIR,
	RuntimeApiMethodParamMetadataIR,
};
use sp_runtime::traits::Block as BlockT;

pub type BlockNumber = u64;
pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<u32, RuntimeCall, (), ()>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type BaseCallFilter = frame_support::traits::Everything;
	type RuntimeOrigin = RuntimeOrigin;
	type Nonce = u64;
	type RuntimeCall = RuntimeCall;
	type Hash = sp_runtime::testing::H256;
	type Hashing = sp_runtime::traits::BlakeTwo256;
	type AccountId = u64;
	type Lookup = sp_runtime::traits::IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

frame_support::construct_runtime!(
	pub enum Runtime
	{
		System: frame_system,
	}
);

sp_api::decl_runtime_apis! {
	/// ApiWithCustomVersion trait documentation
	///
	/// Documentation on multiline.
	#[deprecated]
	pub trait Api {
		fn test(data: u64);
		/// something_with_block.
		fn something_with_block(block: Block) -> Block;
		#[deprecated = "example"]
		fn function_with_two_args(data: u64, block: Block);
		#[deprecated(note = "example", since = "2.0.5")]
		fn same_name();
		#[deprecated(note = "example")]
		fn wild_card(_: u32);
	}
}

// Module to emulate having the implementation in a different file.
mod apis {
	use super::{Block, BlockT, Runtime};

	sp_api::impl_runtime_apis! {
		#[allow(deprecated)]
		impl crate::Api<Block> for Runtime {
			fn test(_data: u64) {
				unimplemented!()
			}

			fn something_with_block(_: Block) -> Block {
				unimplemented!()
			}

			fn function_with_two_args(_: u64, _: Block) {
				unimplemented!()
			}

			fn same_name() {}

			fn wild_card(_: u32) {}
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
}

#[test]
fn runtime_metadata() {
	fn maybe_docs(doc: Vec<&'static str>) -> Vec<&'static str> {
		if cfg!(feature = "no-metadata-docs") {
			vec![]
		} else {
			doc
		}
	}

	let expected_runtime_metadata = vec![
		RuntimeApiMetadataIR {
			name: "Api",
			methods: vec![
				RuntimeApiMethodMetadataIR {
					name: "test",
					inputs: vec![RuntimeApiMethodParamMetadataIR::<MetaForm> {
						name: "data",
						ty: meta_type::<u64>(),
					}],
					output: meta_type::<()>(),
					docs: vec![],
					deprecation_info: DeprecationStatusIR::NotDeprecated,
				},
				RuntimeApiMethodMetadataIR {
					name: "something_with_block",
					inputs: vec![RuntimeApiMethodParamMetadataIR::<MetaForm> {
						name: "block",
						ty: meta_type::<Block>(),
					}],
					output: meta_type::<Block>(),
					docs: maybe_docs(vec![" something_with_block."]),
					deprecation_info: DeprecationStatusIR::NotDeprecated,
				},
				RuntimeApiMethodMetadataIR {
					name: "function_with_two_args",
					inputs: vec![
						RuntimeApiMethodParamMetadataIR::<MetaForm> {
							name: "data",
							ty: meta_type::<u64>(),
						},
						RuntimeApiMethodParamMetadataIR::<MetaForm> {
							name: "block",
							ty: meta_type::<Block>(),
						},
					],
					output: meta_type::<()>(),
					docs: vec![],
					deprecation_info: DeprecationStatusIR::Deprecated {
						note: "example",
						since: None,
					}
				},
				RuntimeApiMethodMetadataIR {
					name: "same_name",
					inputs: vec![],
					output: meta_type::<()>(),
					docs: vec![],
					deprecation_info: DeprecationStatusIR::Deprecated {
						note: "example",
						since: Some("2.0.5"),
					}
			},
				RuntimeApiMethodMetadataIR {
					name: "wild_card",
					inputs: vec![RuntimeApiMethodParamMetadataIR::<MetaForm> {
						name: "__runtime_api_generated_name_0__",
						ty: meta_type::<u32>(),
					}],
					output: meta_type::<()>(),
					docs: vec![],
					deprecation_info: DeprecationStatusIR::Deprecated {
						                    note: "example",
						                    since: None,
						                }
				},
			],
			docs: maybe_docs(vec![
				" ApiWithCustomVersion trait documentation",
				"",
				" Documentation on multiline.",
			]),
			deprecation_info: DeprecationStatusIR::DeprecatedWithoutNote,
			version: codec::Compact(1),

		},
		RuntimeApiMetadataIR {
			name: "Core",
			methods: vec![
				RuntimeApiMethodMetadataIR {
					name: "version",
					inputs: vec![],
					output: meta_type::<sp_version::RuntimeVersion>(),
					docs: maybe_docs(vec![" Returns the version of the runtime."]),
					deprecation_info: DeprecationStatusIR::NotDeprecated,
				},
				RuntimeApiMethodMetadataIR {
					name: "execute_block",
					inputs: vec![RuntimeApiMethodParamMetadataIR::<MetaForm> {
						name: "block",
						ty: meta_type::<Block>(),
					}],
					output: meta_type::<()>(),
					docs: maybe_docs(vec![" Execute the given block."]),
					deprecation_info: DeprecationStatusIR::NotDeprecated,

				},
				RuntimeApiMethodMetadataIR {
					name: "initialize_block",
					inputs: vec![RuntimeApiMethodParamMetadataIR::<MetaForm> {
						name: "header",
						ty: meta_type::<&<Block as BlockT>::Header>(),
					}],
					output: meta_type::<sp_runtime::ExtrinsicInclusionMode>(),
					docs: maybe_docs(vec![" Initialize a block with the given header and return the runtime executive mode."]),
					deprecation_info: DeprecationStatusIR::NotDeprecated,
				},
			],
			docs: maybe_docs(vec![
				" The `Core` runtime api that every Substrate runtime needs to implement.",
			]),
			deprecation_info: DeprecationStatusIR::NotDeprecated,
			version: codec::Compact(5),
		},
	];

	let rt = Runtime;
	let runtime_metadata = (&rt).runtime_metadata();
	pretty_assertions::assert_eq!(runtime_metadata, expected_runtime_metadata);
}
