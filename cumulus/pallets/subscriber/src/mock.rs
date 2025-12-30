// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use super::*;
use cumulus_primitives_core::ParaId;
use frame_support::{derive_impl, parameter_types};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Subscriber: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

// Test handler that records calls
parameter_types! {
	pub static ReceivedData: Vec<(ParaId, Vec<u8>, Vec<u8>)> = vec![];
	pub static TestSubscriptions: Vec<(ParaId, Vec<Vec<u8>>)> = vec![];
}

pub struct TestHandler;
impl SubscriptionHandler for TestHandler {
	fn subscriptions() -> (Vec<(ParaId, Vec<Vec<u8>>)>, Weight) {
		(TestSubscriptions::get(), Weight::zero())
	}

	fn on_data_updated(publisher: ParaId, key: Vec<u8>, value: Vec<u8>) -> Weight {
		ReceivedData::mutate(|d| d.push((publisher, key, value)));
		Weight::zero()
	}
}

parameter_types! {
	pub const MaxPublishers: u32 = 100;
}

impl crate::Config for Test {
	type SubscriptionHandler = TestHandler;
	type WeightInfo = ();
	type MaxPublishers = MaxPublishers;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	t.into()
}
