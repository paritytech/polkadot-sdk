// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use subxt::{OnlineClient, PolkadotConfig};

use self::common::{AhClientEvent, RcClientEvent, TestEventHandler};

pub mod common;

enum TestState {
	// When AH is spawned and AhClient is activated on RC at least a single session change should
	// happen before an election is triggered.
	WaitingForInitialSessionChange,
	/// Create offence and have it reported on RC
	OffenceOnRc,
}

impl TestEventHandler for TestState {
	fn on_ah_client_event(
		&mut self,
		event: AhClientEvent,
		_end_test: &mut Option<tokio::sync::oneshot::Sender<()>>,
	) {
		// match event {
		// 	AhClientEvent::NewValidatorSetCount(new_validator_set_count) => {
		// 		self.handle_new_validator_set_count(new_validator_set_count);
		// 	},
		// }
	}

	fn on_rc_client_event(
		&mut self,
		event: RcClientEvent,
		end_test: &mut Option<tokio::sync::oneshot::Sender<()>>,
	) {
		match event {
			RcClientEvent::SessionReportReceived { activation_time_stamp } => {
				self.handle_ah_session_report_received(activation_time_stamp, end_test);
			},
			_ => {
				//do nothing
			},
		}
	}
}

impl TestState {
	pub fn new() -> Self {
		Self::WaitingForInitialSessionChange
	}

	fn handle_ah_session_report_received(
		&mut self,
		activation_time_stamp: Option<u64>,
		end_test: &mut Option<tokio::sync::oneshot::Sender<()>>,
	) {
		match self {
			TestState::WaitingForInitialSessionChange => {
				log::info!("One session change after activating AH client");
				*self = TestState::OffenceOnRc;
				end_test.take().map(|tx| tx.send(()));
			},
			_ => {
				// do nothing, this event is relevant only in `WaitingForInitialSessionChange` state
			},
		}
	}
}

#[ignore = "Intended for local runs only because it requires manual steps and takes quite a lot of time to execute"]
#[tokio::test(flavor = "multi_thread")]
async fn slashing() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default()
			.filter_or(env_logger::DEFAULT_FILTER_ENV, "info,runtime::staking-async=trace"),
	);

	let config = common::build_network_config().await?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	log::info!("Spawned");

	let rc_node = network.get_node("alice")?;
	let ah_next_node = network.get_node("charlie")?;

	let rc_client: OnlineClient<PolkadotConfig> = rc_node.wait_client().await?;
	let ah_next_client: OnlineClient<PolkadotConfig> = ah_next_node.wait_client().await?;

	let mut test_state = TestState::new();

	common::event_loop(&rc_client, &ah_next_client, &mut test_state).await?;

	log::info!("Create offence");
	common::create_offence(&rc_client).await?;

	common::event_loop(&rc_client, &ah_next_client, &mut test_state).await?;

	Ok(())
}
