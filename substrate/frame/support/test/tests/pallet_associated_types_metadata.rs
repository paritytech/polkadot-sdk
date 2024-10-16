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

use frame_support::{derive_impl, traits::ConstU32};
use scale_info::meta_type;
use sp_metadata_ir::PalletAssociatedTypeMetadataIR;

pub type BlockNumber = u64;
pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<u32, RuntimeCall, (), ()>;

/// Pallet without collectable associated types.
#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		// Runtime events already propagated to the metadata.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		// Constants are already propagated.
		#[pallet::constant]
		type MyGetParam2: Get<u32>;
	}

	#[pallet::event]
	pub enum Event<T: Config> {
		TestEvent,
	}
}

/// Pallet with default collectable associated types.
#[frame_support::pallet]
pub mod pallet2 {
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		// Runtime events already propagated to the metadata.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		// Constants are already propagated.
		#[pallet::constant]
		type MyGetParam2: Get<u32>;

		// Associated type included by default, because it requires TypeInfo bound.
		/// Nonce doc.
		type Nonce: TypeInfo;

		// Associated type included by default, because it requires
		// Parameter bound (indirect TypeInfo).
		type AccountData: Parameter;

		// Associated type without metadata bounds, not included.
		type NotIncluded: From<u8>;
	}

	#[pallet::event]
	pub enum Event<T: Config> {
		TestEvent,
	}
}

/// Pallet with implicit collectable associated types.
#[frame_support::pallet]
pub mod pallet3 {
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// Associated types are not collected by default.
	#[pallet::config(without_automatic_metadata)]
	pub trait Config: frame_system::Config {
		// Runtime events already propagated to the metadata.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		// Constants are already propagated.
		#[pallet::constant]
		type MyGetParam2: Get<u32>;

		// Explicitly include associated types.
		#[pallet::include_metadata]
		type Nonce: TypeInfo;

		type AccountData: Parameter;

		type NotIncluded: From<u8>;
	}

	#[pallet::event]
	pub enum Event<T: Config> {
		TestEvent,
	}
}

impl pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MyGetParam2 = ConstU32<10>;
}

impl pallet2::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MyGetParam2 = ConstU32<10>;
	type Nonce = u64;
	type AccountData = u16;
	type NotIncluded = u8;
}

impl pallet3::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MyGetParam2 = ConstU32<10>;
	type Nonce = u64;
	type AccountData = u16;
	type NotIncluded = u8;
}

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
		Example: pallet,
		DefaultInclusion: pallet2,
		ExplicitInclusion: pallet3,
	}
);

#[test]
fn associated_types_metadata() {
	fn maybe_docs(doc: Vec<&'static str>) -> Vec<&'static str> {
		if cfg!(feature = "no-metadata-docs") {
			vec![]
		} else {
			doc
		}
	}

	let ir = Runtime::metadata_ir();

	// No associated types to collect.
	let pallet = ir.pallets.iter().find(|pallet| pallet.name == "Example").unwrap();
	pretty_assertions::assert_eq!(pallet.associated_types, vec![]);

	// Collect by default types that implement TypeInfo or Parameter.
	let pallet = ir.pallets.iter().find(|pallet| pallet.name == "DefaultInclusion").unwrap();
	pretty_assertions::assert_eq!(
		pallet.associated_types,
		vec![
			PalletAssociatedTypeMetadataIR {
				name: "Nonce",
				ty: meta_type::<u64>(),
				docs: maybe_docs(vec![" Nonce doc."]),
			},
			PalletAssociatedTypeMetadataIR {
				name: "AccountData",
				ty: meta_type::<u16>(),
				docs: vec![],
			}
		]
	);

	// Explicitly include associated types.
	let pallet = ir.pallets.iter().find(|pallet| pallet.name == "ExplicitInclusion").unwrap();
	pretty_assertions::assert_eq!(
		pallet.associated_types,
		vec![PalletAssociatedTypeMetadataIR {
			name: "Nonce",
			ty: meta_type::<u64>(),
			docs: vec![],
		}]
	);

	// Check system pallet.
	let pallet = ir.pallets.iter().find(|pallet| pallet.name == "System").unwrap();
	pretty_assertions::assert_eq!(
		pallet.associated_types,
		vec![
			PalletAssociatedTypeMetadataIR {
				name: "RuntimeCall",
				ty: meta_type::<RuntimeCall>(),
				docs: maybe_docs(vec![" The aggregated `RuntimeCall` type."]),
			},
			PalletAssociatedTypeMetadataIR {
				name: "Nonce",
				ty: meta_type::<u64>(),
				docs: maybe_docs(vec![" This stores the number of previous transactions associated with a sender account."]),
			},
			PalletAssociatedTypeMetadataIR {
				name: "Hash",
				ty: meta_type::<sp_runtime::testing::H256>(),
				docs: maybe_docs(vec![" The output of the `Hashing` function."]),
			},
            PalletAssociatedTypeMetadataIR {
				name: "Hashing",
				ty: meta_type::<sp_runtime::traits::BlakeTwo256>(),
				docs: maybe_docs(vec![" The hashing system (algorithm) being used in the runtime (e.g. Blake2)."]),
			},
            PalletAssociatedTypeMetadataIR {
                name: "AccountId",
                ty: meta_type::<u64>(),
                docs: maybe_docs(vec![" The user account identifier type for the runtime."]),
            },
            PalletAssociatedTypeMetadataIR {
                name: "Block",
                ty: meta_type::<Block>(),
                docs: maybe_docs(vec![
                    " The Block type used by the runtime. This is used by `construct_runtime` to retrieve the",
                    " extrinsics or other block specific data as needed.",
                ]),
            },
            PalletAssociatedTypeMetadataIR {
                name: "AccountData",
                ty: meta_type::<()>(),
                docs: maybe_docs(vec![
                    " Data to be associated with an account (other than nonce/transaction counter, which this",
                    " pallet does regardless).",
                ]),
            },
		]
	);
}
