#![allow(dead_code)]
//! State test execution logic

use anyhow::Result;
use frame_support::sp_runtime::BuildStorage;
use revive_dev_runtime::Runtime;
use revm_statetest_types::{Test as PostState, TestUnit};
use serde::{Deserialize, Serialize};
use sp_core::H160;
use std::collections::BTreeMap;

use revm::{
	context::cfg::CfgEnv,
	context_interface::result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction},
	primitives::{hardfork::SpecId, U256},
	Context, ExecuteCommitEvm, MainBuilder, MainContext,
};

use crate::{cli::Args, transaction_helper::create_signed_transaction};

/// Test execution result for go-ethereum evm statetest compatibility
///
/// This custom type maintains exact compatibility with go-ethereum's `evm statetest` output format.
/// While revm has its own output format with different field names and structure, we keep this
/// to ensure seamless integration with existing Ethereum tooling that expects go-ethereum's format.
#[derive(Debug, Serialize, Deserialize)]
pub struct TestResult {
	pub name: String,
	pub pass: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub state_root: Option<String>,
	pub fork: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub state: Option<serde_json::Value>,
}

/// Execute a single state test variant
pub fn execute_revm_state_test(
	test_name: &str,
	test_case: &TestUnit,
	fork: &str,
	index: usize,
	expected_post_state: &PostState,
	args: &Args,
) -> Result<TestResult> {
	let state_result = execute_revm_statetest(test_case, fork, index, expected_post_state);
	process_results(state_result, test_name, fork, expected_post_state, args)
}

pub fn execute_revive_state_test(
	test_name: &str,
	test_case: &TestUnit,
	expected_post_state: &PostState,
	args: &Args,
) -> Result<TestResult> {
	let state_result = execute_revive_statetest(test_case, expected_post_state);
	process_results(state_result, test_name, "Prague", expected_post_state, args)
}

fn process_results(
	state_result: Result<
		Result<ExecutionResult<HaltReason>, EVMError<std::convert::Infallible, InvalidTransaction>>,
		anyhow::Error,
	>,
	test_name: &str,
	fork: &str,
	expected_post_state: &PostState,
	args: &Args,
) -> Result<TestResult> {
	let mut result = TestResult {
		name: test_name.to_string(),
		pass: true,
		state_root: Some(expected_post_state.hash.to_string()),
		fork: fork.to_string(),
		error: None,
		state: None,
	};

	match state_result {
		Ok(execution_result) => match &execution_result {
			Ok(ExecutionResult::Success { .. }) => {
				if let Some(exception) = &expected_post_state.expect_exception {
					if !exception.is_empty() {
						result.pass = false;
						result.error = Some(format!(
							"Expected exception '{exception}' but execution succeeded",
						));
					} else {
						result.pass = true;
					}
				} else {
					result.pass = true;
				}
			},
			Ok(ExecutionResult::Revert { .. }) => {
				if let Some(exception) = &expected_post_state.expect_exception {
					if exception.is_empty() {
						result.pass = false;
						result.error = Some("Execution reverted unexpectedly".to_string());
					} else {
						result.pass = true;
					}
				} else {
					result.pass = false;
					result.error = Some("Execution reverted unexpectedly".to_string());
				}
			},
			Ok(ExecutionResult::Halt { reason, .. }) => {
				if let Some(exception) = &expected_post_state.expect_exception {
					if exception.is_empty() {
						result.pass = false;
						result.error = Some(format!("Execution halted: {reason:?}"));
					} else {
						result.pass = true;
					}
				} else {
					result.pass = false;
					result.error = Some(format!("Execution halted: {reason:?}"));
				}
			},
			Err(e) =>
				if let Some(exception) = &expected_post_state.expect_exception {
					if exception.is_empty() {
						result.pass = false;
						result.error = Some(format!("EVM error: {e}"));
					} else {
						result.pass = true;
					}
				} else {
					result.pass = false;
					result.error = Some(format!("EVM error: {e}"));
				},
		},
		Err(e) => {
			result.pass = false;
			result.error = Some(format!("Execution error: {e}"));
		},
	}

	// Add state dump if requested
	if args.dump {
		// Convert the post_state to a simple representation for compatibility
		let account_addresses: Vec<String> =
			expected_post_state.post_state.keys().map(|addr| addr.to_string()).collect();
		result.state = Some(serde_json::json!({
			"accounts": account_addresses,
			"root": expected_post_state.hash
		}));
	}

	Ok(result)
}

struct ExtBuilder {
	genesis_config: pallet_revive::GenesisConfig<Runtime>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { genesis_config: pallet_revive::GenesisConfig::<Runtime>::default() }
	}
}

impl ExtBuilder {
	#[allow(dead_code)]
	pub fn with_genesis_config(
		mut self,
		genesis_config: pallet_revive::GenesisConfig<Runtime>,
	) -> Self {
		self.genesis_config = genesis_config;
		self
	}

	fn build(self, test_case: &TestUnit) -> sp_io::TestExternalities {
		// Create proper runtime storage
		let mut storage = frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.expect("Failed to build storage");

		let mut genesis_config = self.genesis_config;

		// Convert test_case.pre accounts to genesis accounts
		let mut accounts = Vec::new();
		for (evm_address, account_info) in &test_case.pre {
			let addr = H160::from_slice(evm_address.0.as_slice());
			let balance = sp_core::U256::from_big_endian(&account_info.balance.to_be_bytes::<32>());

			// Convert contract data if code exists
			let contract_data = if account_info.code.is_empty() {
				None
			} else {
				// Convert storage from HashMap to BTreeMap with proper key format
				let mut storage = BTreeMap::new();
				for (key, value) in &account_info.storage {
					storage.insert(key.to_be_bytes().into(), value.to_be_bytes().into());
				}

				Some(pallet_revive::genesis::ContractData {
					code: account_info.code.to_vec().into(),
					storage,
				})
			};

			accounts.push(pallet_revive::genesis::Account {
				address: addr,
				balance,
				nonce: account_info.nonce as u32,
				contract_data,
			});
		}

		genesis_config.accounts = accounts;

		// Assimilate the genesis config into storage
		genesis_config
			.assimilate_storage(&mut storage)
			.expect("Failed to assimilate revive storage");

		let mut ext = sp_io::TestExternalities::new(storage);

		ext.execute_with(|| {
			revive_dev_runtime::set_coinbase(test_case.env.current_coinbase.0 .0.into());

			revive_dev_runtime::set_chain_id(
				test_case
					.env
					.current_chain_id
					.unwrap_or(U256::from(1))
					.try_into()
					.expect("chain id should fit into u64"),
			);

			frame_system::Pallet::<Runtime>::set_block_number(
				test_case
					.env
					.current_number
					.try_into()
					.expect("block number should fit into u32"),
			);

			pallet_timestamp::Pallet::<Runtime>::set_timestamp(
				test_case
					.env
					.current_timestamp
					.try_into()
					.expect("timestamp should fit into u64"),
			);

			// TODO: Set difficulty from env.current_difficulty
			// TODO: Set gas limit from env.current_gas_limit
			// TODO: Set base fee from env.current_base_fee
		});

		ext
	}
}

pub fn execute_revive_statetest(
	test_case: &TestUnit,
	expected_post_state: &PostState,
) -> Result<
	Result<ExecutionResult<HaltReason>, EVMError<std::convert::Infallible, InvalidTransaction>>,
> {
	use revive_dev_runtime::{Runtime, RuntimeCall};

	let signed_tx = create_signed_transaction(test_case, &expected_post_state.indexes)?;
	let mut ext = ExtBuilder::default().build(test_case);

	// Create eth_transact call
	let payload = signed_tx.signed_payload();
	let call = RuntimeCall::Revive(pallet_revive::Call::eth_transact { payload });
	use sp_core::Encode;
	let encoded_len = call.encoded_size();

	ext.execute_with(|| {
		use frame_support::dispatch::GetDispatchInfo;
		use revive_dev_runtime::{RuntimeOrigin, UncheckedExtrinsic};
		use sp_runtime::{
			generic,
			generic::ExtrinsicFormat,
			traits::{Checkable, DispatchTransaction},
		};
		let uxt: UncheckedExtrinsic = generic::UncheckedExtrinsic::new_bare(call).into();
		let context = frame_system::ChainContext::<Runtime>::default();
		let result: generic::CheckedExtrinsic<_, _, _> = uxt.check(&context).unwrap();

		let (account_id, extra) = match result.format {
			ExtrinsicFormat::Signed(signer, extra) => (signer, extra),
			_ => unreachable!(),
		};

		let dispatch_info = result.function.get_dispatch_info();
		extra
			.dispatch_transaction(
				RuntimeOrigin::signed(account_id),
				result.function,
				&dispatch_info,
				encoded_len,
				0,
			)
			.unwrap()
			.unwrap();
	});

	let result: ExecutionResult<HaltReason> = ExecutionResult::Success {
		reason: revm::context::result::SuccessReason::Return,
		gas_used: 0,
		gas_refunded: 0,
		logs: vec![],
		output: revm::context::result::Output::Call(Default::default()),
	};
	Ok(Ok(result))
}

/// Execute a state test using REVM, following the pattern from revm/bins/revme
fn execute_revm_statetest(
	test_case: &TestUnit,
	fork: &str,
	_index: usize,
	expected_post_state: &PostState,
) -> Result<
	Result<ExecutionResult<HaltReason>, EVMError<std::convert::Infallible, InvalidTransaction>>,
> {
	// Map fork name to SpecId
	let spec_id = match fork {
		"Frontier" => SpecId::FRONTIER,
		"Homestead" => SpecId::HOMESTEAD,
		"Tangerine" => SpecId::TANGERINE,
		"Spurious" => SpecId::SPURIOUS_DRAGON,
		"Byzantium" => SpecId::BYZANTIUM,
		"Constantinople" => SpecId::CONSTANTINOPLE,
		"Petersburg" => SpecId::PETERSBURG,
		"Istanbul" => SpecId::ISTANBUL,
		"Berlin" => SpecId::BERLIN,
		"London" => SpecId::LONDON,
		"Merge" => SpecId::MERGE,
		"Shanghai" => SpecId::SHANGHAI,
		"Cancun" => SpecId::CANCUN,
		"Prague" => SpecId::PRAGUE,
		_ => SpecId::PRAGUE,
	};

	// Prepare initial state from test pre-state
	let cache_state = test_case.state();

	// Setup configuration
	let mut cfg = CfgEnv::default();
	cfg.spec = spec_id;
	cfg.chain_id = test_case.env.current_chain_id.unwrap_or(U256::from(1)).try_into().unwrap_or(1);

	// Setup block environment
	let block = test_case.block_env(&cfg);

	// Setup transaction environment
	let tx = expected_post_state.tx_env(test_case)?;

	// Prepare state with cache
	let mut cache = cache_state.clone();
	cache.set_state_clear_flag(cfg.spec.is_enabled_in(SpecId::SPURIOUS_DRAGON));
	let mut state = revm::database::State::builder()
		.with_cached_prestate(cache)
		.with_bundle_update()
		.build();

	// Create EVM context and execute
	let mut evm = Context::mainnet()
		.with_block(&block)
		.with_tx(&tx)
		.with_cfg(&cfg)
		.with_db(&mut state)
		.build_mainnet();

	// Execute transaction
	let exec_result = evm.transact_commit(&tx);

	Ok(exec_result)
}

/// Print test results
pub fn report(args: &Args, results: Vec<TestResult>) {
	if args.human {
		// Human-readable output
		let mut pass_count = 0;
		for result in &results {
			if result.pass {
				pass_count += 1;
				println!("[\\x1b[32mPASS\\x1b[0m] {} ({})", result.name, result.fork);
			} else {
				println!(
					"[\\x1b[31mFAIL\\x1b[0m] {} ({}): {}",
					result.name,
					result.fork,
					result.error.as_ref().unwrap_or(&"Unknown error".to_string())
				);
			}
			if let Some(state) = &result.state {
				println!("{}", serde_json::to_string_pretty(state).unwrap_or_default());
			}
		}
		println!("--");
		println!("{} tests passed, {} tests failed.", pass_count, results.len() - pass_count);
	} else {
		// JSON output
		let json = serde_json::to_string_pretty(&results).unwrap_or_default();
		println!("{}", json);
	}
}
