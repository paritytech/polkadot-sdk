//! Transaction helper utilities

use anyhow::{Context, Result};
use pallet_revive::{
	evm::{
		Account, Byte, Bytes as ReviveBytes, GenericTransaction, InputOrData, TransactionSigned,
	},
	U256Converter,
};
use revm_statetest_types::{recover_address, TestUnit, TxPartIndices};

/// Convert a StateTest transaction to a GenericTransaction from pallet-revive
///
/// This function aligns with the logic found in:
/// - go-ethereum's `toMessage` function in tests/state_test_util.go
/// - revm's `tx_env` function in crates/statetest-types/src/test.rs
pub fn create_generic_transaction(
	test: &TestUnit,
	indices: &TxPartIndices,
) -> Result<pallet_revive::evm::GenericTransaction> {
	// Extract transaction data for the specific test case
	let gas_limit =
		test.transaction.gas_limit.get(indices.gas).context("Invalid gas limit index")?;
	let value = test.transaction.value.get(indices.value).context("Invalid value index")?;
	let data = test.transaction.data.get(indices.data).context("Invalid data index")?;

	// Convert revm types to pallet-revive types
	let chain_id = test.env.current_chain_id.map(|v| pallet_revive::evm::U256::from_revm_u256(&v));
	let gas = Some(pallet_revive::evm::U256::from_revm_u256(gas_limit));
	let gas_price =
		test.transaction.gas_price.map(|v| pallet_revive::evm::U256::from_revm_u256(&v));
	let max_fee_per_gas = test
		.transaction
		.max_fee_per_gas
		.map(|v| pallet_revive::evm::U256::from_revm_u256(&v));
	let max_priority_fee_per_gas = test
		.transaction
		.max_priority_fee_per_gas
		.map(|v| pallet_revive::evm::U256::from_revm_u256(&v));
	let max_fee_per_blob_gas = test
		.transaction
		.max_fee_per_blob_gas
		.map(|v| pallet_revive::evm::U256::from_revm_u256(&v));
	let nonce = Some(pallet_revive::evm::U256::from_revm_u256(&test.transaction.nonce));
	let value = Some(pallet_revive::evm::U256::from_revm_u256(value));
	// Handle recipient - following go-ethereum pattern
	let to = test
		.transaction
		.to
		.map(|addr| pallet_revive::evm::Address::from_slice(addr.as_slice()));

	// Handle sender - following go-ethereum/revm pattern
	let from = if let Some(sender_addr) = test.transaction.sender {
		// Use explicit sender if provided
		Some(pallet_revive::evm::Address::from_slice(sender_addr.as_slice()))
	} else {
		// Derive sender from secret key following revm pattern
		recover_address(test.transaction.secret_key.as_slice())
			.map(|addr| pallet_revive::evm::Address::from_slice(addr.as_slice()))
	};

	// Convert input data
	let input = InputOrData::from(ReviveBytes(data.to_vec()));

	// Convert access list if present
	let access_list = test
		.transaction
		.access_lists
		.get(indices.data)
		.cloned()
		.flatten()
		.map(|list| convert_access_list(&list));

	// Convert blob versioned hashes
	let blob_versioned_hashes = test
		.transaction
		.blob_versioned_hashes
		.iter()
		.map(|hash| pallet_revive::evm::H256::from_slice(hash.as_slice()))
		.collect();

	// Determine transaction type
	let tx_type = test.transaction.tx_type(indices.data).map(|t| Byte(t as u8));

	Ok(GenericTransaction {
		access_list,
		blob_versioned_hashes,
		blobs: Vec::new(), // State tests don't typically include raw blob data
		chain_id,
		from,
		gas,
		gas_price,
		input,
		max_fee_per_blob_gas,
		max_fee_per_gas,
		max_priority_fee_per_gas,
		nonce,
		to,
		r#type: tx_type,
		value,
		authorization_list: vec![],
	})
}

/// Create a signed transaction from a StateTest fixture using the test's secret key
///
/// This function extracts the secret key from the StateTest transaction and creates
/// an Account to sign the transaction, eliminating the need to pass an external account.
pub fn create_signed_transaction(
	test: &TestUnit,
	indices: &TxPartIndices,
) -> Result<TransactionSigned> {
	// Create GenericTransaction using existing function
	let generic_tx = create_generic_transaction(test, indices)?;

	// Convert to appropriate TransactionUnsigned variant
	let unsigned_tx = generic_tx.try_into_unsigned().map_err(|_| {
		anyhow::anyhow!("Failed to convert GenericTransaction to TransactionUnsigned")
	})?;

	// Create account from the secret key in the test case
	let secret_key: [u8; 32] = test.transaction.secret_key.0;
	let account = Account::from_secret_key(secret_key);

	// Sign using Account.sign_transaction()
	Ok(account.sign_transaction(unsigned_tx))
}

/// Convert revm AccessList to pallet-revive AccessList
fn convert_access_list(
	list: &revm::context_interface::transaction::AccessList,
) -> Vec<pallet_revive::evm::AccessListEntry> {
	use pallet_revive::evm::AccessListEntry;

	list.0
		.iter()
		.map(|item| AccessListEntry {
			address: pallet_revive::evm::Address::from_slice(item.address.as_slice()),
			storage_keys: item
				.storage_keys
				.iter()
				.map(|hash| pallet_revive::evm::H256::from_slice(hash.as_slice()))
				.collect(),
		})
		.collect()
}
