use codec::{Decode, Encode};
use serde::Deserialize;
use sp_core::H160;
use sp_runtime::{
	traits::{Block as BlockT, Zero},
	transaction_validity::TransactionPriority,
};

/// Provides transaction details required by `TransactionPriorityModifier`.
pub trait TransactionDetailProvider: Send + Sync {
	type Block: BlockT;
	fn get_transaction_detail(
		&self,
		tx: &<Self::Block as BlockT>::Extrinsic,
	) -> Option<TransactionDetail>;
}

/// Implemented for the `Client`, to get tx priorities from the transaction pool.
pub trait TransactionPriorityModifierT {
	type Block: BlockT;
	fn get_priority(&self, tx: &<Self::Block as BlockT>::Extrinsic) -> Option<TransactionPriority>;
}

/// Main struct for getting TX priority from the specified list.
pub struct TransactionPriorityModifier<Block> {
	priority_list: Vec<TransactionPriorityItem>,
	// dynamic dispatch here. This can be changed to use a generic type, but this would require
	// way more changes in the existing code.
	pub tx_detail_provider: Box<dyn TransactionDetailProvider<Block = Block>>,
}

impl<Block: BlockT> TransactionPriorityModifier<Block> {
	/// Parameters:
	///  `json_data`: JSON string with `Vec<TransactionPriorityItem>`.
	///     Can be set to `None` if no transaction priorities should be overwritten.
	///  `tx_detail_provider`: Type providing `TransactionDetail`.
	pub fn new(
		json_data: Option<&'static str>,
		tx_detail_provider: Box<dyn TransactionDetailProvider<Block = Block>>,
	) -> Self {
		let data = if let Some(data_str) = json_data {
			// it's up to the implementors to make sure that the list doesn't contain duplicate
			// entries.
			serde_json::from_str(&data_str).unwrap()
		} else {
			Vec::new()
		};

		Self { priority_list: data, tx_detail_provider }
	}

	pub fn get_priority(&self, utx: &<Block as BlockT>::Extrinsic) -> Option<TransactionPriority> {
		let Some(user_tx) = self.tx_detail_provider.get_transaction_detail(utx) else {
			return None;
		};

		let priority_txs: Vec<TransactionPriorityItem> = self
			.priority_list
			.clone()
			.into_iter()
			.filter(|item| item.module == user_tx.module && item.extrinsic == user_tx.extrinsic)
			.collect();

		if priority_txs.len().is_zero() {
			return None;
		}

		let Some(TransactionTypeDetail::Evm(user_tx_data)) = user_tx.transaction_data else {
			return None;
		};

		let Some(priority_tx) = priority_txs.iter().find(|p_tx| {
			let Some(TransactionTypeDetail::Evm(priority_tx_data)) = &p_tx.transaction_data else {
				return false;
			};

			priority_tx_data.signer == user_tx_data.signer &&
				priority_tx_data.call_address == user_tx_data.call_address
		}) else {
			return None;
		};

		Some(priority_tx.priority)
	}
}

/// Transaction details specific for an EVM transaction
#[derive(Clone, Encode, Decode, Deserialize, Debug)]
pub struct EvmTransactionDetail {
	pub call_address: Option<H160>,
	pub signer: H160,
}

#[derive(Clone, Encode, Decode, Deserialize, Debug)]
pub enum TransactionTypeDetail {
	Evm(EvmTransactionDetail),
}

/// Transaction details used to determine transaction's priority.
#[derive(Clone, Debug)]
pub struct TransactionDetail {
	pub module: &'static str,
	pub extrinsic: &'static str,
	pub transaction_data: Option<TransactionTypeDetail>,
}

/// Data type of the `tx_priority_list` json file.
#[derive(Clone, Encode, Decode, Deserialize, Debug)]
pub struct TransactionPriorityItem {
	module: String,
	extrinsic: String,
	transaction_data: Option<TransactionTypeDetail>,
	priority: TransactionPriority,
}

#[cfg(test)]
mod tests {
	use hex_literal::hex;
	use sp_runtime::{generic, traits::BlakeTwo256, OpaqueExtrinsic};

	use super::*;
	type BlockNumber = u32;

	const JSON_DATA: &str = "[{\"module\":\"Ethereum\",\"extrinsic\":\"transact\",\"transaction_data\":{\"Evm\":{\"call_address\":\"0xdee629af973ebf5bf261ace12ffd1900ac715f5e\",\"signer\":\"0x33a5e905fB83FcFB62B0Dd1595DfBc06792E054e\"}},\"priority\":1},{\"module\":\"Ethereum\",\"extrinsic\":\"transact\",\"transaction_data\":{\"Evm\":{\"call_address\":\"0x48ae7803cd09c48434e3fc5629f15fb76f0b5ce5\",\"signer\":\"0xff0c624016c873d359dde711b42a2f475a5a07d3\"}},\"priority\":2}]";

	struct TestExtrinsics {
		valid_caller_and_signer_1th: OpaqueExtrinsic,
		valid_caller_and_signer_2nd: OpaqueExtrinsic,
		valid_caller_invalid_signer: OpaqueExtrinsic,
		invalid_caller_valid_signer: OpaqueExtrinsic,
		invalid_caller_and_signer: OpaqueExtrinsic,
		timestamp_set: OpaqueExtrinsic,
	}

	impl Default for TestExtrinsics {
		fn default() -> Self {
			Self {
				valid_caller_and_signer_1th: OpaqueExtrinsic::from_bytes(
					&hex!["1001020304"].to_vec(),
				)
				.unwrap(),
				valid_caller_and_signer_2nd: OpaqueExtrinsic::from_bytes(
					&hex!["1001020305"].to_vec(),
				)
				.unwrap(),
				valid_caller_invalid_signer: OpaqueExtrinsic::from_bytes(
					&hex!["1001020306"].to_vec(),
				)
				.unwrap(),
				invalid_caller_valid_signer: OpaqueExtrinsic::from_bytes(
					&hex!["1001020307"].to_vec(),
				)
				.unwrap(),
				invalid_caller_and_signer: OpaqueExtrinsic::from_bytes(
					&hex!["1001020308"].to_vec(),
				)
				.unwrap(),
				timestamp_set: OpaqueExtrinsic::from_bytes(&hex!["1001020309"].to_vec()).unwrap(),
			}
		}
	}

	struct DummyTxProvider;
	impl TransactionDetailProvider for DummyTxProvider {
		type Block = generic::Block<generic::Header<BlockNumber, BlakeTwo256>, OpaqueExtrinsic>;

		fn get_transaction_detail(
			&self,
			tx: &<Self::Block as BlockT>::Extrinsic,
		) -> Option<TransactionDetail> {
			let exts = TestExtrinsics::default();

			if *tx == exts.valid_caller_and_signer_1th {
				return Some(TransactionDetail {
					module: "Ethereum",
					extrinsic: "transact",
					transaction_data: Some(TransactionTypeDetail::Evm(EvmTransactionDetail {
						call_address: Some(H160(hex!["dee629af973ebf5bf261ace12ffd1900ac715f5e"])),
						signer: H160(hex!["33a5e905fB83FcFB62B0Dd1595DfBc06792E054e"]),
					})),
				});
			} else if *tx == exts.valid_caller_and_signer_2nd {
				return Some(TransactionDetail {
					module: "Ethereum",
					extrinsic: "transact",
					transaction_data: Some(TransactionTypeDetail::Evm(EvmTransactionDetail {
						call_address: Some(H160(hex!["48ae7803cd09c48434e3fc5629f15fb76f0b5ce5"])),
						signer: H160(hex!["ff0c624016c873d359dde711b42a2f475a5a07d3"]),
					})),
				});
			} else if *tx == exts.valid_caller_invalid_signer {
				return Some(TransactionDetail {
					module: "Ethereum",
					extrinsic: "transact",
					transaction_data: Some(TransactionTypeDetail::Evm(EvmTransactionDetail {
						call_address: Some(H160(hex!["48ae7803cd09c48434e3fc5629f15fb76f0b5ce5"])),
						signer: H160(hex!["c2f4a370440ef0e662f36e67c6014122582869d8"]),
					})),
				});
			} else if *tx == exts.invalid_caller_valid_signer {
				return Some(TransactionDetail {
					module: "Ethereum",
					extrinsic: "transact",
					transaction_data: Some(TransactionTypeDetail::Evm(EvmTransactionDetail {
						call_address: Some(H160(hex!["c2f4a370440ef0e662f36e67c6014122582869d8"])),
						signer: H160(hex!["ff0c624016c873d359dde711b42a2f475a5a07d3"]),
					})),
				});
			} else if *tx == exts.invalid_caller_and_signer {
				return Some(TransactionDetail {
					module: "Ethereum",
					extrinsic: "transact",
					transaction_data: Some(TransactionTypeDetail::Evm(EvmTransactionDetail {
						call_address: Some(H160(hex!["c2f4a370440ef0e662f36e67c6014122582869d8"])),
						signer: H160(hex!["465351ddc79e04663aec9e86b56c95b85278deef"]),
					})),
				});
			} else if *tx == exts.timestamp_set {
				return Some(TransactionDetail {
					module: "Timestamp",
					extrinsic: "set",
					transaction_data: None,
				});
			}

			return None;
		}
	}

	#[test]
	fn get_priority_should_work_for_1th_item_in_list() {
		let tm = TransactionPriorityModifier::new(Some(JSON_DATA), Box::new(DummyTxProvider));

		let exts = TestExtrinsics::default();

		assert_eq!(tm.get_priority(&exts.valid_caller_and_signer_1th), Some(1));
	}

	#[test]
	fn get_priority_should_work_for_2nd_item_in_list() {
		let tm = TransactionPriorityModifier::new(Some(JSON_DATA), Box::new(DummyTxProvider));
		let exts = TestExtrinsics::default();

		assert_eq!(tm.get_priority(&exts.valid_caller_and_signer_2nd), Some(2));
	}

	#[test]
	fn get_priority_should_return_none_when_caller_is_valid_and_signer_is_invalid() {
		let tm = TransactionPriorityModifier::new(Some(JSON_DATA), Box::new(DummyTxProvider));
		let exts = TestExtrinsics::default();

		assert_eq!(tm.get_priority(&exts.valid_caller_invalid_signer), None);
	}

	#[test]
	fn get_priority_should_return_none_when_caller_is_invalid_and_signer_is_valid() {
		let tm = TransactionPriorityModifier::new(Some(JSON_DATA), Box::new(DummyTxProvider));

		let exts = TestExtrinsics::default();

		assert_eq!(tm.get_priority(&exts.invalid_caller_valid_signer), None);
	}

	#[test]
	fn get_priority_should_return_none_when_caller_and_signer_are_invalid() {
		let tm = TransactionPriorityModifier::new(Some(JSON_DATA), Box::new(DummyTxProvider));

		let exts = TestExtrinsics::default();

		assert_eq!(tm.get_priority(&exts.invalid_caller_and_signer), None);
	}

	#[test]
	fn get_priority_should_return_none_when_tx_doesnt_match_tx_in_list() {
		let tm = TransactionPriorityModifier::new(Some(JSON_DATA), Box::new(DummyTxProvider));

		let exts = TestExtrinsics::default();

		assert_eq!(tm.get_priority(&exts.timestamp_set), None);
	}
}
