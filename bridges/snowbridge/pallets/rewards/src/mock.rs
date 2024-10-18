// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use codec::Encode;
use frame_support::{derive_impl, parameter_types};
use hex_literal::hex;
use sp_core::{ConstU32, H160};
use sp_runtime::{
	traits::{IdentifyAccount, IdentityLookup, Verify},
	BuildStorage, MultiSignature,
};
use sp_std::{convert::From, default::Default};
use xcm::{latest::SendXcm, prelude::*};

use crate::{self as snowbridge_pallet_rewards};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Storage, Event<T>},
		EthereumRewards: snowbridge_pallet_rewards::{Pallet, Call, Storage, Event<T>},
	}
);

pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;


#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

parameter_types! {
	pub EthereumNetwork: NetworkId = NetworkId::Ethereum { chain_id: 11155111 };
	pub WethAddress: H160 = hex!("774667629726ec1FaBEbCEc0D9139bD1C8f72a23").into();
}

impl snowbridge_pallet_rewards::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AssetHubParaId = ConstU32<1000>;
	type EthereumNetwork = EthereumNetwork;
	type WethAddress = WethAddress;
	type XcmSender = MockXcmSender;
	type WeightInfo = ();
}

// Mock XCM sender that always succeeds
pub struct MockXcmSender;

impl SendXcm for MockXcmSender {
	type Ticket = Xcm<()>;

	fn validate(
		dest: &mut Option<Location>,
		xcm: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		if let Some(location) = dest {
			match location.unpack() {
				(_, [Parachain(1001)]) => return Err(XcmpSendError::NotApplicable),
				_ => Ok((xcm.clone().unwrap(), Assets::default())),
			}
		} else {
			Ok((xcm.clone().unwrap(), Assets::default()))
		}
	}

	fn deliver(xcm: Self::Ticket) -> core::result::Result<XcmHash, XcmpSendError> {
		let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
		Ok(hash)
	}
}

pub const WETH: u128 = 1_000_000_000_000_000_000;

pub fn last_events(n: usize) -> Vec<RuntimeEvent> {
	frame_system::Pallet::<Test>::events()
		.into_iter()
		.rev()
		.take(n)
		.rev()
		.map(|e| e.event)
		.collect()
}

pub fn expect_events(e: Vec<RuntimeEvent>) {
	assert_eq!(last_events(e.len()), e);
}

pub fn new_tester() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let ext = sp_io::TestExternalities::new(t);
	ext
}
