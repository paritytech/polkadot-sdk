// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use self::common::{AhClientEvent, RcClientEvent, TestEventHandler};
use subxt::{OnlineClient, PolkadotConfig};

pub mod common;

enum TestState {
	// When AH is spawned and AhClient is activated on RC at least a single session change should
	// happen before an election is triggered.
	WaitingForInitialSessionChange,
	// Wait for an election to complete. We get into this state after the first session change and
	// move on when we receive a page containing `Ok(0)` as a result.
	WaitingForElectionResult,
	// The validator set should be delivered to RC
	WaitForNewValidatorSetCount,
	// AH should receive a session report with an activation timestamp
	WaitForSessionReportWithActivationTimestamp { elapsed_sessions: usize },
}

impl TestEventHandler for TestState {
	fn on_ah_client_event(
		&mut self,
		event: AhClientEvent,
		_end_test: &mut Option<tokio::sync::oneshot::Sender<()>>,
	) {
		match event {
			AhClientEvent::NewValidatorSetCount(new_validator_set_count) => {
				self.handle_new_validator_set_count(new_validator_set_count);
			},
		}
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
			RcClientEvent::PagedElectionProceeded { page_idx, page_content } => {
				self.handle_ah_paged_election_proceeded(page_idx, page_content);
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
				*self = TestState::WaitingForElectionResult;
			},
			TestState::WaitingForElectionResult => {
				// ignore
				log::info!("SessionReport in WaitingForElectionResult - ignoring");
			},
			TestState::WaitForNewValidatorSetCount => {
				// ignore
				log::info!("SessionReport in WaitForNewValidatorSetCount - ignoring");
			},
			TestState::WaitForSessionReportWithActivationTimestamp { elapsed_sessions } => {
				if activation_time_stamp.is_some() {
					log::info!(
						"Got session report with activation timestamp: {:?}",
						activation_time_stamp
					);
					// All done. Terminate the test.
					end_test.take().unwrap().send(()).unwrap();
					return
				}
				*elapsed_sessions += 1;

				if *elapsed_sessions > 6 {
					assert!(
						activation_time_stamp.is_some(),
						"Expected activation time stamp within 6 sessions"
					);
				}
			},
		}
	}

	fn handle_ah_paged_election_proceeded(&mut self, page_idx: u32, page_content: Result<u32, ()>) {
		match self {
			TestState::WaitingForInitialSessionChange => {
				assert!(false, "PagedElectionProceeded before the first session change?");
			},
			TestState::WaitingForElectionResult => {
				log::info!(
					"Paged election proceeded: page_idx: {}, page_content: {:?}",
					page_idx,
					page_content
				);

				// we can be smarter here and avoid the magic numbers
				if page_idx == 3 {
					assert!(page_content.is_ok(), "Expected Ok");
					assert!(page_content.unwrap() == 10, "Expected 500");
				} else if page_idx == 2 {
					assert!(page_content.is_ok(), "Expected Ok");
					assert!(page_content.unwrap() == 0, "Expected 0");

					// at this point it's safe to assume that the election is complete
					// we will get more `PagedElectionProceeded` with `Err` but we'll just ignore
					// them
					*self = TestState::WaitForNewValidatorSetCount;
					log::info!("Election complete, waiting for validator set on RC");
				} else {
					assert!(page_content.is_err(), "Expected Err for page {}", page_idx);
				}
			},
			TestState::WaitForNewValidatorSetCount => {
				// as per the comment above - ignore these
			},
			TestState::WaitForSessionReportWithActivationTimestamp { elapsed_sessions: _ } => {
				// ignore
			},
		}
	}

	fn handle_new_validator_set_count(&mut self, new_validator_set_count: u32) {
		match self {
			TestState::WaitingForInitialSessionChange => {
				assert!(false, "New validator set count before the first session change?");
			},
			TestState::WaitingForElectionResult => {
				assert!(false, "New validator set count before the election is complete?");
			},
			TestState::WaitForNewValidatorSetCount => {
				log::info!("Got NewValidatorSetCount on RC: {}", new_validator_set_count);
				assert!(new_validator_set_count == 10, "Expected a validator set count of 10");
				*self =
					TestState::WaitForSessionReportWithActivationTimestamp { elapsed_sessions: 0 };
			},
			TestState::WaitForSessionReportWithActivationTimestamp { elapsed_sessions: _ } => {
				// ignore
				log::info!("Got NewValidatorSetCount in WaitForSessionReportWithActivationTimestamp - ignoring");
			},
		}
	}
}

#[ignore = "Intended for local runs only because it requires manual steps and takes quite a lot of time to execute"]
#[tokio::test(flavor = "multi_thread")]
async fn happy_case() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
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

	Ok(())
}
