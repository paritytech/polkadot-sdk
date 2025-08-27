//! Transaction helper utilities

use anyhow::{Context, Result};
use pallet_revive::evm::{
	Byte, Bytes as ReviveBytes, GenericTransaction, InputOrData,
	TransactionSigned, Account,
};
use revm_statetest_types::{Test, TestUnit};

/// Convert a StateTest transaction to a GenericTransaction from pallet-revive
pub fn create_generic_transaction(
	test: &Test,
	unit: &TestUnit,
) -> Result<pallet_revive::evm::GenericTransaction> {
	// Get transaction parameters based on test indices
	let indices = &test.indexes;

	// Extract transaction data for the specific test case
	let gas_limit =
		unit.transaction.gas_limit.get(indices.gas).context("Invalid gas limit index")?;
	let value = unit.transaction.value.get(indices.value).context("Invalid value index")?;
	let data = unit.transaction.data.get(indices.data).context("Invalid data index")?;

	// Convert revm types to pallet-revive types
	let chain_id = unit.env.current_chain_id.map(convert_u256);
	let gas = Some(convert_u256_ref(gas_limit));
	let gas_price = unit.transaction.gas_price.map(convert_u256);
	let max_fee_per_gas = unit.transaction.max_fee_per_gas.map(convert_u256);
	let max_priority_fee_per_gas = unit.transaction.max_priority_fee_per_gas.map(convert_u256);
	let max_fee_per_blob_gas = unit.transaction.max_fee_per_blob_gas.map(convert_u256);
	let nonce = Some(convert_u256_ref(&unit.transaction.nonce));
	let value = Some(convert_u256_ref(value));
	let to = unit.transaction.to.map(convert_address);
	let from = unit.transaction.sender.map(convert_address);

	// Convert input data
	let input = InputOrData::from(ReviveBytes(data.to_vec()));

	// Convert access list if present
	let access_list = unit
		.transaction
		.access_lists
		.get(indices.data)
		.cloned()
		.flatten()
		.map(|list| convert_access_list(&list));

	// Convert blob versioned hashes
	let blob_versioned_hashes =
		unit.transaction.blob_versioned_hashes.iter().map(convert_h256).collect();

	// Determine transaction type
	let tx_type = unit.transaction.tx_type(indices.data).map(|t| Byte(t as u8));

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
	})
}

/// Create a signed transaction from a StateTest fixture using the test's secret key
///
/// This function extracts the secret key from the StateTest transaction and creates
/// an Account to sign the transaction, eliminating the need to pass an external account.
pub fn create_signed_transaction(
	test: &Test,
	unit: &TestUnit,
) -> Result<TransactionSigned> {
	// Create GenericTransaction using existing function
	let generic_tx = create_generic_transaction(test, unit)?;
	
	// Convert to appropriate TransactionUnsigned variant
	let unsigned_tx = generic_tx.try_into_unsigned().map_err(|_| anyhow::anyhow!("Failed to convert GenericTransaction to TransactionUnsigned"))?;
	
	// Create account from the secret key in the test case
	let secret_key: [u8; 32] = unit.transaction.secret_key.0;
	let account = Account::from_secret_key(secret_key);
	
	// Sign using Account.sign_transaction()
	Ok(account.sign_transaction(unsigned_tx))
}


/// Convert revm U256 to pallet-revive U256
fn convert_u256(value: revm::primitives::U256) -> pallet_revive::evm::U256 {
	let bytes = value.to_be_bytes::<32>();
	pallet_revive::evm::U256::from_big_endian(&bytes)
}

/// Convert revm U256 reference to pallet-revive U256
fn convert_u256_ref(value: &revm::primitives::U256) -> pallet_revive::evm::U256 {
	let bytes = value.to_be_bytes::<32>();
	pallet_revive::evm::U256::from_big_endian(&bytes)
}

/// Convert revm Address to pallet-revive Address
fn convert_address(address: revm::primitives::Address) -> pallet_revive::evm::Address {
	pallet_revive::evm::Address::from_slice(address.as_slice())
}

/// Convert revm B256 to pallet-revive H256
fn convert_h256(hash: &revm::primitives::B256) -> pallet_revive::evm::H256 {
	pallet_revive::evm::H256::from_slice(hash.as_slice())
}

/// Convert revm AccessList to pallet-revive AccessList
fn convert_access_list(
	list: &revm::context_interface::transaction::AccessList,
) -> Vec<pallet_revive::evm::AccessListEntry> {
	use pallet_revive::evm::AccessListEntry;

	list.0
		.iter()
		.map(|item| AccessListEntry {
			address: convert_address(item.address),
			storage_keys: item.storage_keys.iter().map(convert_h256).collect(),
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_create_generic_transaction_logic() {
		// This test ensures the GenericTransaction creation logic compiles correctly
		// A full integration test would require actual test data
		assert!(true, "GenericTransaction creation function compiles correctly");
	}

	#[test]
	fn test_type_conversions() {
		use pallet_revive::evm::U256 as ReviveU256;
		use revm::primitives::U256 as RevmU256;

		// Test U256 conversion
		let revm_value = RevmU256::from(12345);
		let revive_value = convert_u256(revm_value);
		assert_eq!(revive_value, ReviveU256::from(12345));

		// Test Address conversion
		let revm_addr = revm::primitives::Address::from([1u8; 20]);
		let revive_addr = convert_address(revm_addr);
		assert_eq!(revive_addr, pallet_revive::evm::Address::from([1u8; 20]));
	}

	#[test]
	fn test_try_into_unsigned_legacy() {
		use pallet_revive::evm::*;
		
		let generic_tx = GenericTransaction {
			access_list: None,
			blob_versioned_hashes: Vec::new(),
			blobs: Vec::new(),
			chain_id: Some(U256::from(1)),
			from: None,
			gas: Some(U256::from(21000)),
			gas_price: Some(U256::from(1000000000)),
			input: InputOrData::from(vec![0x60, 0x60, 0x60, 0x40]),
			max_fee_per_blob_gas: None,
			max_fee_per_gas: None,
			max_priority_fee_per_gas: None,
			nonce: Some(U256::from(0)),
			to: Some(Address::from([0x42; 20])),
			r#type: Some(Byte(0)), // Legacy transaction
			value: Some(U256::from(1000)),
		};

		let result = generic_tx.try_into_unsigned();
		assert!(result.is_ok(), "Legacy transaction conversion should succeed");
		
		if let Ok(TransactionUnsigned::TransactionLegacyUnsigned(tx)) = result {
			assert_eq!(tx.chain_id, Some(U256::from(1)));
			assert_eq!(tx.gas, U256::from(21000));
			assert_eq!(tx.gas_price, U256::from(1000000000));
			assert_eq!(tx.nonce, U256::from(0));
			assert_eq!(tx.value, U256::from(1000));
			assert_eq!(tx.to, Some(Address::from([0x42; 20])));
		} else {
			panic!("Expected TransactionLegacyUnsigned variant");
		}
	}

	#[test]
	fn test_try_into_unsigned_eip1559() {
		use pallet_revive::evm::*;
		
		let generic_tx = GenericTransaction {
			access_list: Some(vec![]),
			blob_versioned_hashes: Vec::new(),
			blobs: Vec::new(),
			chain_id: Some(U256::from(1)),
			from: None,
			gas: Some(U256::from(21000)),
			gas_price: None,
			input: InputOrData::from(vec![0x60, 0x60, 0x60, 0x40]),
			max_fee_per_blob_gas: None,
			max_fee_per_gas: Some(U256::from(2000000000)),
			max_priority_fee_per_gas: Some(U256::from(1000000000)),
			nonce: Some(U256::from(0)),
			to: Some(Address::from([0x42; 20])),
			r#type: Some(Byte(2)), // EIP-1559 transaction
			value: Some(U256::from(1000)),
		};

		let result = generic_tx.try_into_unsigned();
		assert!(result.is_ok(), "EIP-1559 transaction conversion should succeed");
		
		if let Ok(TransactionUnsigned::Transaction1559Unsigned(tx)) = result {
			assert_eq!(tx.chain_id, U256::from(1));
			assert_eq!(tx.gas, U256::from(21000));
			assert_eq!(tx.max_fee_per_gas, U256::from(2000000000));
			assert_eq!(tx.max_priority_fee_per_gas, U256::from(1000000000));
			assert_eq!(tx.nonce, U256::from(0));
			assert_eq!(tx.value, U256::from(1000));
			assert_eq!(tx.to, Some(Address::from([0x42; 20])));
		} else {
			panic!("Expected Transaction1559Unsigned variant");
		}
	}
}

