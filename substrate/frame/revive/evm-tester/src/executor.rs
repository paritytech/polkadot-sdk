#![allow(dead_code)]
//! State test execution logic

use anyhow::Result;
use frame_support::sp_runtime::BuildStorage;
use revive_dev_runtime::Runtime;
use revm_statetest_types::{Test, TestUnit};
use serde::{Deserialize, Serialize};
use sp_core::H160;
use std::collections::BTreeMap;

use revm::{
	context::cfg::CfgEnv,
	context_interface::result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction},
	primitives::{hardfork::SpecId, keccak256, Log, U256},
	Context, Database, ExecuteCommitEvm, MainBuilder, MainContext,
};

use crate::{cli::Args, transaction_helper::create_signed_transaction};

/// Custom error for state test verification failures
#[derive(Debug)]
pub enum StateTestError {
	ExecutionError(anyhow::Error),
	EvmError(EVMError<std::convert::Infallible, InvalidTransaction>),
	LogsRootMismatch {
		got: revm::primitives::B256,
		expected: revm::primitives::B256,
	},
	AccountBalanceMismatch {
		address: revm::primitives::Address,
		got: U256,
		expected: U256,
	},
	AccountNonceMismatch {
		address: revm::primitives::Address,
		got: u64,
		expected: u64,
	},
	AccountCodeMismatch {
		address: revm::primitives::Address,
	},
	AccountStorageMismatch {
		address: revm::primitives::Address,
		key: U256,
		got: U256,
		expected: U256,
	},
	UnexpectedException {
		expected: Option<String>,
		got: Option<String>,
	},
}

impl std::fmt::Display for StateTestError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			StateTestError::ExecutionError(e) => write!(f, "Execution error: {e}"),
			StateTestError::EvmError(e) => write!(f, "EVM error: {e}"),
			StateTestError::LogsRootMismatch { got, expected } =>
				write!(f, "Logs root mismatch: got {got:?}, expected {expected:?}"),
			StateTestError::AccountBalanceMismatch { address, got, expected } => write!(
				f,
				"Account balance mismatch for {address}: got {got:?}, expected {expected:?}",
			),
			StateTestError::AccountNonceMismatch { address, got, expected } => write!(
				f,
				"Account nonce mismatch for {address}: got {got:?}, expected {expected:?}",
			),
			StateTestError::AccountCodeMismatch { address } =>
				write!(f, "Account code mismatch for {address}"),
			StateTestError::AccountStorageMismatch { address, key, got, expected } => write!(
        f,
        "Account storage mismatch for {address}, slot {key:?}: got {got:?}, expected {expected:?}",
        ),
			StateTestError::UnexpectedException { expected, got } =>
				write!(f, "Unexpected exception: got {got:?}, expected {expected:?}"),
		}
	}
}

impl std::error::Error for StateTestError {}

/// Compute logs root hash by RLP encoding logs and computing Keccak256 hash
fn compute_logs_root(logs: &[Log]) -> revm::primitives::B256 {
	let mut out = Vec::new();
	alloy_rlp::encode_list(logs, &mut out);
	keccak256(&out)
}

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
	expected_post_state: &Test,
	args: &Args,
) -> Result<TestResult> {
	let state_result = execute_revm_statetest(test_case, fork, index, expected_post_state);
	process_results(state_result, test_name, fork, expected_post_state, args)
}

pub fn execute_revive_state_test(
	test_name: &str,
	test_case: &TestUnit,
	expected_post_state: &Test,
	args: &Args,
) -> Result<TestResult> {
	let state_result = execute_revive_statetest(test_case, expected_post_state);
	process_results(state_result, test_name, "Prague", expected_post_state, args)
}

fn process_results(
	state_result: Result<Result<ExecutionResult<HaltReason>, StateTestError>, anyhow::Error>,
	test_name: &str,
	fork: &str,
	expected_post_state: &Test,
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
		Ok(execution_result) => match execution_result {
			Ok(_) => {
				// Execution succeeded and all verifications passed
				result.pass = true;
			},
			Err(state_test_error) => {
				// Verification failed or expected error occurred
				result.pass = false;
				result.error = Some(state_test_error.to_string());
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

pub fn execute_revive_statetest_old(
	test_case: &TestUnit,
	expected_post_state: &Test,
) -> Result<Result<ExecutionResult<HaltReason>, StateTestError>> {
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

	// TODO fix
	let result: ExecutionResult<HaltReason> = ExecutionResult::Success {
		reason: revm::context::result::SuccessReason::Return,
		gas_used: 0,
		gas_refunded: 0,
		logs: vec![],
		output: revm::context::result::Output::Call(Default::default()),
	};
	Ok(Ok(result))
}

pub fn execute_revive_statetest(
	test_case: &TestUnit,
	expected_post_state: &Test,
) -> Result<Result<ExecutionResult<HaltReason>, StateTestError>> {
	use crate::transaction_helper::create_generic_transaction;
	use pallet_revive::evm::DryRunConfig;
	use revive_dev_runtime::Runtime;

	let tx = create_generic_transaction(test_case, &expected_post_state.indexes)?;
	let mut ext = ExtBuilder::default().build(test_case);

	// Execute transaction and capture results
	let (execution_result, logs) = ext.execute_with(|| {
		let tracer_type = pallet_revive::evm::TracerType::CallTracer(Some(
			pallet_revive::evm::CallTracerConfig { with_logs: true, only_top_call: false },
		));
		let mut tracer = pallet_revive::Pallet::<Runtime>::evm_tracer(tracer_type.clone());
		let t = tracer.as_tracing();

		let dry_run_config = DryRunConfig::new(None);
		let result = pallet_revive::tracing::trace(t, || {
			pallet_revive::Pallet::<Runtime>::dry_run_eth_transact(tx, dry_run_config)
		});
		// TODO: Extract actual logs from the execution
		// For now, return empty logs as placeholder
		let logs = vec![];
		(result, logs)
	});

	let result: ExecutionResult<HaltReason> = match execution_result {
		Ok(_) => ExecutionResult::Success {
			reason: revm::context::result::SuccessReason::Return,
			gas_used: 0, // Untested for now since we have a different gas model
			gas_refunded: 0,
			logs,
			// TODO: Extract actual output
			output: revm::context::result::Output::Call(Default::default()),
		},
		Err(_) => {
			// TODO: Map pallet errors to appropriate ExecutionResult variants
			ExecutionResult::Halt {
				reason: HaltReason::OutOfGas(revm::context::result::OutOfGasError::Basic), /* Placeholder */
				gas_used: 0,
			}
		},
	};

	// Perform verification similar to REVM version
	match &result {
		ExecutionResult::Success { logs, .. } => {
			dbg!(&logs);
			// Check for expected exceptions
			if let Some(exception) = &expected_post_state.expect_exception {
				if !exception.is_empty() {
					return Ok(Err(StateTestError::UnexpectedException {
						expected: Some(exception.clone()),
						got: None,
					}));
				}
			}

			// TODO this is going to require scc block work
			let actual_logs_root = compute_logs_root(logs);
			if actual_logs_root != expected_post_state.logs {
				return Ok(Err(StateTestError::LogsRootMismatch {
					got: actual_logs_root,
					expected: expected_post_state.logs,
				}));
			}

			for (_address, _expected_account) in &expected_post_state.post_state {
				// Convert EVM address to Substrate AccountId
				// Query account state from Substrate storage
				// Compare balance, nonce, code, storage
				// Return StateTestError on mismatch
			}

			Ok(Ok(result))
		},
		ExecutionResult::Revert { .. } => {
			// Check if revert was expected
			if expected_post_state.expect_exception.is_none() ||
				expected_post_state.expect_exception.as_ref().map_or(true, |e| e.is_empty())
			{
				return Ok(Err(StateTestError::UnexpectedException {
					expected: None,
					got: Some("Execution reverted".to_string()),
				}));
			}
			Ok(Ok(result))
		},
		ExecutionResult::Halt { reason, .. } => {
			// Check if halt was expected
			if expected_post_state.expect_exception.is_none() ||
				expected_post_state.expect_exception.as_ref().map_or(true, |e| e.is_empty())
			{
				return Ok(Err(StateTestError::UnexpectedException {
					expected: None,
					got: Some(format!("Execution halted: {:?}", reason)),
				}));
			}
			Ok(Ok(result))
		},
	}
}

/// Execute a state test using REVM, following the pattern from revm/bins/revme
fn execute_revm_statetest(
	test_case: &TestUnit,
	fork: &str,
	_index: usize,
	expected_post_state: &Test,
) -> Result<Result<ExecutionResult<HaltReason>, StateTestError>> {
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

	// Perform verification if execution succeeded
	match &exec_result {
		Ok(ExecutionResult::Success { logs, .. }) => {
			// Check for expected exceptions
			if let Some(exception) = &expected_post_state.expect_exception {
				if !exception.is_empty() {
					return Ok(Err(StateTestError::UnexpectedException {
						expected: Some(exception.clone()),
						got: None,
					}));
				}
			}

			// Verify logs root
			let actual_logs_root = compute_logs_root(logs);
			if actual_logs_root != expected_post_state.logs {
				return Ok(Err(StateTestError::LogsRootMismatch {
					got: actual_logs_root,
					expected: expected_post_state.logs,
				}));
			}

			// Verify account states
			for (address, expected_account) in &expected_post_state.post_state {
				let Ok(actual_account) = state.load_cache_account(*address);
				if let Some(account) = &actual_account.account {
					// Verify balance
					if account.info.balance != expected_account.balance {
						return Ok(Err(StateTestError::AccountBalanceMismatch {
							address: *address,
							got: account.info.balance,
							expected: expected_account.balance,
						}));
					}

					// Verify nonce
					if account.info.nonce != expected_account.nonce {
						return Ok(Err(StateTestError::AccountNonceMismatch {
							address: *address,
							got: account.info.nonce,
							expected: expected_account.nonce,
						}));
					}

					// Verify code
					let code_hash = account.info.code_hash;
					let actual_code = state.code_by_hash(code_hash).unwrap_or_default();
					if actual_code.bytecode() != expected_account.code.as_ref() {
						return Ok(Err(StateTestError::AccountCodeMismatch { address: *address }));
					}

					// Verify storage
					for (storage_key, expected_value) in &expected_account.storage {
						let actual_value =
							state.storage(*address, (*storage_key).into()).unwrap_or_default();
						if actual_value != *expected_value {
							return Ok(Err(StateTestError::AccountStorageMismatch {
								address: *address,
								key: (*storage_key).into(),
								got: actual_value,
								expected: *expected_value,
							}));
						}
					}
				}
			}

			Ok(exec_result.map_err(|e| StateTestError::EvmError(e)))
		},
		Ok(ExecutionResult::Revert { .. }) => {
			// Check if revert was expected
			if expected_post_state.expect_exception.is_none() ||
				expected_post_state.expect_exception.as_ref().map_or(true, |e| e.is_empty())
			{
				return Ok(Err(StateTestError::UnexpectedException {
					expected: None,
					got: Some("Execution reverted".to_string()),
				}));
			}
			Ok(exec_result.map_err(|e| StateTestError::EvmError(e)))
		},
		Ok(ExecutionResult::Halt { reason, .. }) => {
			// Check if halt was expected
			if expected_post_state.expect_exception.is_none() ||
				expected_post_state.expect_exception.as_ref().map_or(true, |e| e.is_empty())
			{
				return Ok(Err(StateTestError::UnexpectedException {
					expected: None,
					got: Some(format!("Execution halted: {:?}", reason)),
				}));
			}
			Ok(exec_result.map_err(|e| StateTestError::EvmError(e)))
		},
		Err(e) => {
			// Check if error was expected
			if expected_post_state.expect_exception.is_none() ||
				expected_post_state.expect_exception.as_ref().map_or(true, |e| e.is_empty())
			{
				Ok(Err(StateTestError::EvmError(e.clone())))
			} else {
				Ok(exec_result.map_err(|e| StateTestError::EvmError(e)))
			}
		},
	}
}

/// Print test results
pub fn report(args: &Args, results: Vec<TestResult>) {
	if args.human {
		// Human-readable output
		let mut pass_count = 0;
		for result in &results {
			if result.pass {
				pass_count += 1;
				println!("[\x1b[32mPASS\x1b[0m] {} ({})", result.name, result.fork);
			} else {
				println!(
					"[\x1b[31mFAIL\x1b[0m] {} ({}): {}",
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
