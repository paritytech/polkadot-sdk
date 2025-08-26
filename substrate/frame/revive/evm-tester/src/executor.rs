//! State test execution logic

use anyhow::Result;
use frame_support::sp_runtime::BuildStorage;
use revive_dev_runtime::Runtime;
use revm_statetest_types::{Test as PostState, TestUnit as StateTest};
use serde::{Deserialize, Serialize};
use sp_core::H160;

use revm::{
	context::cfg::CfgEnv,
	context_interface::result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction},
	primitives::{hardfork::SpecId, U256},
	Context, ExecuteCommitEvm, MainBuilder, MainContext,
};

use crate::cli::Args;

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
pub fn execute_state_test(
	test_name: &str,
	test_case: &StateTest,
	fork: &str,
	index: usize,
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

	// Parse transaction variant for this index
	let tx = &test_case.transaction;
	let _gas_limit_idx = index.min(tx.gas_limit.len() - 1);
	let _value_idx = index.min(tx.value.len() - 1);
	let _data_idx = index.min(tx.data.len() - 1);

	// Execute the state test using REVM
	match execute_revm_statetest(test_case, fork, index, expected_post_state) {
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
	existential_deposit: u64,
	genesis_config: pallet_revive::GenesisConfig<Runtime>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			existential_deposit: 1, // Minimal existential deposit for testing
			genesis_config: pallet_revive::GenesisConfig::<Runtime>::default(),
		}
	}
}

impl ExtBuilder {
	#[allow(unused)]
	fn existential_deposit(mut self, existential_deposit: u64) -> Self {
		self.existential_deposit = existential_deposit;
		self
	}

	fn with_genesis_config(
		mut self,
		genesis_config: pallet_revive::GenesisConfig<Runtime>,
	) -> Self {
		self.genesis_config = genesis_config;
		self
	}

	fn build(self, test_case: &StateTest) -> sp_io::TestExternalities {
		// Create proper runtime storage
		let mut storage = frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.expect("Failed to build storage");

		// Prepare externally owned EVM accounts from test_case.pre
		let mut externally_owned_accounts = Vec::new();

		for (evm_address, account_info) in &test_case.pre {
			let addr = H160::from_slice(evm_address.0.as_slice());
			let value = sp_core::U256::from_big_endian(&account_info.balance.to_be_bytes::<32>());

			externally_owned_accounts.push((addr, value));

			// TODO: Set up contract code if account_info.code is not empty
			// TODO: Set up storage for contracts
			// TODO: Set nonce appropriately
		}

		// Create genesis config with EVM accounts from test case
		let mut genesis_config = self.genesis_config;
		genesis_config.externally_owned_accounts = externally_owned_accounts;

		// Assimilate the genesis config into storage
		genesis_config
			.assimilate_storage(&mut storage)
			.expect("Failed to assimilate revive storage");

		let mut ext = sp_io::TestExternalities::new(storage);

		ext.execute_with(|| {
			// Set up block environment from test_case.env
			// TODO: Set block number from env.current_number
			// TODO: Set block timestamp from env.current_timestamp
			// TODO: Set coinbase/author from env.current_coinbase
			// TODO: Set difficulty from env.current_difficulty
			// TODO: Set gas limit from env.current_gas_limit
			// TODO: Set base fee from env.current_base_fee

			// For now, just ensure the system is initialized
			frame_system::Pallet::<Runtime>::set_block_number(1u32.into());
		});

		ext
	}
}

/// 3. Add a function to create a Signed Transaction from the test_case
/// 4. like in ../src/evm/runtime.rs let's create a eth_transact call from the eth_transact
/// 5. then we will create a CheckedExtrinsic and check the call to get a dispatchable
/// 6. TODO execute the call and process the outcome
#[allow(dead_code)]
fn execute_revive_statetest(
	test_case: &StateTest,
	_index: usize,
	_expected_post_state: &PostState,
) -> Result<
	Result<ExecutionResult<HaltReason>, EVMError<std::convert::Infallible, InvalidTransaction>>,
> {
	let _ext_builder = ExtBuilder::default().build(test_case);

	// TODO: Steps 3-6
	// - Extract transaction from test_case at the given index
	// - Create eth_transact call
	// - Create CheckedExtrinsic
	// - Execute and process outcome

	todo!("Complete implementation of steps 3-6")
}

/// Execute a state test using REVM, following the pattern from revm/bins/revme
fn execute_revm_statetest(
	test_case: &StateTest,
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
		_ => SpecId::CANCUN, // Default to Cancun (latest stable)
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
