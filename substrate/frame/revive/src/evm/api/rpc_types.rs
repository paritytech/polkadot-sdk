//! Utility impl for the RPC types.
use super::{ReceiptInfo, TransactionInfo, TransactionSigned};

impl TransactionInfo {
	/// Create a new [`TransactionInfo`] from a receipt and a signed transaction.
	pub fn new(receipt: ReceiptInfo, transaction_signed: TransactionSigned) -> Self {
		Self {
			block_hash: receipt.block_hash,
			block_number: receipt.block_number,
			from: receipt.from,
			hash: receipt.transaction_hash,
			transaction_index: receipt.transaction_index,
			transaction_signed,
		}
	}
}
