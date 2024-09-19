//! Various adapters for the RPC types.
use super::{
	Bytes, GenericTransaction, ReceiptInfo, TransactionInfo, TransactionLegacySigned,
	TransactionLegacyUnsigned, TransactionSigned,
};
use codec::Encode;
use pallet_revive::EthInstantiateInput;
use sp_core::{H160, U256};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

impl TransactionLegacyUnsigned {
	/// Convert a legacy transaction to a [`GenericTransaction`].
	pub fn as_generic(self, from: H160) -> GenericTransaction {
		GenericTransaction {
			from: Some(from),
			chain_id: self.chain_id,
			gas: Some(self.gas),
			input: Some(self.input),
			nonce: Some(self.nonce),
			to: self.to,
			r#type: Some(self.r#type.as_byte()),
			value: Some(self.value),
			..Default::default()
		}
	}

	/// Build a transaction from an instantiate call.
	pub fn from_instantiate(
		input: EthInstantiateInput,
		value: U256,
		gas_price: U256,
		gas: U256,
		nonce: U256,
		chain_id: U256,
	) -> Self {
		Self {
			input: Bytes(input.encode()),
			value,
			gas_price,
			gas,
			nonce,
			chain_id: Some(chain_id),
			..Default::default()
		}
	}

	/// Build a transaction from a call.
	pub fn from_call(
		to: H160,
		input: Vec<u8>,
		value: U256,
		gas_price: U256,
		gas: U256,
		nonce: U256,
		chain_id: U256,
	) -> Self {
		Self {
			to: Some(to),
			input: Bytes(input),
			value,
			gas_price,
			gas,
			nonce,
			chain_id: Some(chain_id),
			..Default::default()
		}
	}
}

// TODO: store the transaction_signed in the cache so that we can populate `transaction_signed`
impl From<ReceiptInfo> for TransactionInfo {
	fn from(receipt: ReceiptInfo) -> Self {
		Self {
			block_hash: receipt.block_hash,
			block_number: receipt.block_number,
			from: receipt.from,
			hash: receipt.transaction_hash,
			transaction_index: receipt.transaction_index,
			transaction_signed: TransactionSigned::TransactionLegacySigned(
				TransactionLegacySigned::default(),
			),
		}
	}
}
