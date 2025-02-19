// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

mod parachain;
mod relay_chain;

#[cfg(test)]
mod tests;

use sp_runtime::BuildStorage;
use sp_tracing::{self, tracing_subscriber};
use std::{
	io::Write,
	sync::{Arc, Mutex},
};
use tracing_subscriber::fmt::MakeWriter;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([1u8; 32]);
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

/// A reusable log capturing struct for unit tests.
/// Captures logs written during test execution for assertions.
pub struct LogCapture {
	buffer: Arc<Mutex<Vec<u8>>>,
}

impl LogCapture {
	/// Creates a new `LogCapture` instance with an internal buffer.
	pub fn new() -> Self {
		LogCapture { buffer: Arc::new(Mutex::new(Vec::new())) }
	}

	/// Retrieves the captured logs as a `String`.
	pub fn get_logs(&self) -> String {
		String::from_utf8(self.buffer.lock().unwrap().clone()).unwrap()
	}

	/// Returns a clone of the internal buffer for use in `MakeWriter`.
	pub fn writer(&self) -> Self {
		LogCapture { buffer: Arc::clone(&self.buffer) }
	}
}

impl Write for LogCapture {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		let mut logs = self.buffer.lock().unwrap();
		logs.extend_from_slice(buf);
		Ok(buf.len())
	}

	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}

impl<'a> MakeWriter<'a> for LogCapture {
	type Writer = Self;

	/// Provides a `MakeWriter` implementation for `tracing_subscriber`.
	fn make_writer(&'a self) -> Self::Writer {
		self.writer()
	}
}

/// Runs a test block with logging enabled and captures logs for assertions.
/// Usage:
/// ```ignore
/// let log_capture = run_with_logging!({
///     my_test_function();
/// });
/// assert_logs_contain!(log_capture, "Expected log message");
/// ```
#[macro_export]
macro_rules! run_with_logging {
	($test:block) => {{
		let log_capture = LogCapture::new();
		let subscriber = tracing_subscriber::fmt().with_writer(log_capture.writer()).finish();

		tracing::subscriber::with_default(subscriber, || $test);

		log_capture
	}};
}

/// Macro to assert that captured logs contain a specific substring.
/// Usage:
/// ```ignore
/// assert_logs_contain!(log_capture, "Expected log message");
/// ```
#[macro_export]
macro_rules! assert_logs_contain {
	($log_capture:expr, $expected:expr) => {
		let logs = $log_capture.get_logs();
		assert!(
			logs.contains($expected),
			"Expected '{}' in logs, but logs were:\n{}",
			$expected,
			logs
		);
	};
}

decl_test_parachain! {
	pub struct ParaA {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = parachain::MsgQueue,
		DmpMessageHandler = parachain::MsgQueue,
		new_ext = para_ext(1),
	}
}

decl_test_parachain! {
	pub struct ParaB {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = parachain::MsgQueue,
		DmpMessageHandler = parachain::MsgQueue,
		new_ext = para_ext(2),
	}
}

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = relay_chain::Runtime,
		RuntimeCall = relay_chain::RuntimeCall,
		RuntimeEvent = relay_chain::RuntimeEvent,
		XcmConfig = relay_chain::XcmConfig,
		MessageQueue = relay_chain::MessageQueue,
		System = relay_chain::System,
		new_ext = relay_ext(),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(1, ParaA),
			(2, ParaB),
		],
	}
}

pub fn parent_account_id() -> parachain::AccountId {
	let location = (Parent,);
	parachain::location_converter::LocationConverter::convert_location(&location.into()).unwrap()
}

pub fn child_account_id(para: u32) -> relay_chain::AccountId {
	let location = (Parachain(para),);
	relay_chain::location_converter::LocationConverter::convert_location(&location.into()).unwrap()
}

pub fn child_account_account_id(para: u32, who: sp_runtime::AccountId32) -> relay_chain::AccountId {
	let location = (Parachain(para), AccountId32 { network: None, id: who.into() });
	relay_chain::location_converter::LocationConverter::convert_location(&location.into()).unwrap()
}

pub fn sibling_account_account_id(para: u32, who: sp_runtime::AccountId32) -> parachain::AccountId {
	let location = (Parent, Parachain(para), AccountId32 { network: None, id: who.into() });
	parachain::location_converter::LocationConverter::convert_location(&location.into()).unwrap()
}

pub fn parent_account_account_id(who: sp_runtime::AccountId32) -> parachain::AccountId {
	let location = (Parent, AccountId32 { network: None, id: who.into() });
	parachain::location_converter::LocationConverter::convert_location(&location.into()).unwrap()
}

pub fn para_ext(para_id: u32) -> sp_io::TestExternalities {
	use parachain::{MsgQueue, Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(ALICE, INITIAL_BALANCE), (parent_account_id(), INITIAL_BALANCE)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		sp_tracing::try_init_simple();
		System::set_block_number(1);
		MsgQueue::set_para_id(para_id.into());
	});
	ext
}

pub fn relay_ext() -> sp_io::TestExternalities {
	use relay_chain::{Runtime, RuntimeOrigin, System, Uniques};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(child_account_id(1), INITIAL_BALANCE),
			(child_account_id(2), INITIAL_BALANCE),
		],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		assert_eq!(Uniques::force_create(RuntimeOrigin::root(), 1, ALICE, true), Ok(()));
		assert_eq!(Uniques::mint(RuntimeOrigin::signed(ALICE), 1, 42, child_account_id(1)), Ok(()));
	});
	ext
}

pub type RelayChainPalletXcm = pallet_xcm::Pallet<relay_chain::Runtime>;
pub type ParachainPalletXcm = pallet_xcm::Pallet<parachain::Runtime>;
