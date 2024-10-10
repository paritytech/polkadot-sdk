//! Utilities for working with Ethereum accounts.
use crate::{
	evm::{TransactionLegacySigned, TransactionLegacyUnsigned},
	H160,
};
use rlp::Encodable;

/// A simple account that can sign transactions
pub struct Account(subxt_signer::eth::Keypair);

impl Default for Account {
	fn default() -> Self {
		Self(subxt_signer::eth::dev::alith())
	}
}

impl From<subxt_signer::eth::Keypair> for Account {
	fn from(kp: subxt_signer::eth::Keypair) -> Self {
		Self(kp)
	}
}

impl Account {
	/// Get the [`H160`] address of the account.
	pub fn address(&self) -> H160 {
		H160::from_slice(&self.0.account_id().as_ref())
	}

	/// Sign a transaction.
	pub fn sign_transaction(&self, tx: TransactionLegacyUnsigned) -> TransactionLegacySigned {
		let rlp_encoded = tx.rlp_bytes();
		let signature = self.0.sign(&rlp_encoded);
		TransactionLegacySigned::from(tx, signature.as_ref())
	}
}
