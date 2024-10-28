//! Transaction extensions are, briefly, a means for different chains to extend the "basic"
//! extrinsic format with custom data that can be checked by the runtime.
//!
//! # FRAME provided transaction extensions
//!
//! FRAME by default already provides the following transaction extensions:
//!
//! - [`CheckGenesis`](frame_system::CheckGenesis): Ensures that a transaction was sent for the same
//!   network. Determined based on genesis.
//!
//! - [`CheckMortality`](frame_system::CheckMortality): Extends a transaction with a configurable
//!   mortality.
//!
//! - [`CheckNonZeroSender`](frame_system::CheckNonZeroSender): Ensures that the sender of a
//!   transaction is not the *all zero account* (all bytes of the accountid are zero).
//!
//! - [`CheckNonce`](frame_system::CheckNonce): Extends a transaction with a nonce to prevent replay
//!   of transactions and to provide ordering of transactions.
//!
//! - [`CheckSpecVersion`](frame_system::CheckSpecVersion): Ensures that a transaction was built for
//!   the currently active runtime.
//!
//! - [`CheckTxVersion`](frame_system::CheckTxVersion): Ensures that the transaction signer used the
//!   correct encoding of the call.
//!
//! - [`CheckWeight`](frame_system::CheckWeight): Ensures that the transaction fits into the block
//!   before dispatching it.
//!
//! - [`ChargeTransactionPayment`](pallet_transaction_payment::ChargeTransactionPayment): Charges
//!   transaction fees from the signer based on the weight of the call using the native token.
//!
//! - [`ChargeAssetTxPayment`](pallet_asset_tx_payment::ChargeAssetTxPayment): Charges transaction
//!   fees from the signer based on the weight of the call using any supported asset (including the
//!   native token).
//!
//! - [`ChargeAssetTxPayment`(using
//!   conversion)](pallet_asset_conversion_tx_payment::ChargeAssetTxPayment): Charges transaction
//!   fees from the signer based on the weight of the call using any supported asset (including the
//!   native token). The asset is converted to the native token using a pool.
//!
//! - [`SkipCheckIfFeeless`](pallet_skip_feeless_payment::SkipCheckIfFeeless): Allows transactions
//!   to be processed without paying any fee. This requires that the `call` that should be
//!   dispatched is augmented with the [`feeless_if`](frame_support::pallet_macros::feeless_if)
//!   attribute.
//!
//! - [`CheckMetadataHash`](frame_metadata_hash_extension::CheckMetadataHash): Extends transactions
//!   to include the so-called metadata hash. This is required by chains to support the generic
//!   Ledger application and other similar offline wallets.
//!
//! - [`StorageWeightReclaim`](cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim): A
//!   transaction extension for parachains that reclaims unused storage weight after executing a
//!   transaction.
//!
//! For more information about these extensions, follow the link to the type documentation.
//!
//! # Building a custom transaction extension
//!
//! Defining a couple of very simple transaction extensions looks like the following:
#![doc = docify::embed!("./src/reference_docs/transaction_extensions.rs", transaction_extensions_example)]

#[docify::export]
pub mod transaction_extensions_example {
	use codec::{Decode, Encode};
	use scale_info::TypeInfo;
	use sp_runtime::{
		impl_tx_ext_default,
		traits::{Dispatchable, TransactionExtension},
		transaction_validity::TransactionValidityError,
	};

	// This doesn't actually check anything, but simply allows
	// some arbitrary `u32` to be added to the extrinsic payload
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToPayload(pub u32);

	impl<Call: Dispatchable> TransactionExtension<Call> for AddToPayload {
		const IDENTIFIER: &'static str = "AddToPayload";
		type Implicit = ();
		type Pre = ();
		type Val = ();

		impl_tx_ext_default!(Call; weight validate prepare);
	}

	// This is the opposite; nothing will be added to the extrinsic payload,
	// but the Implicit type (`1234u32`) will be added to the
	// payload to be signed.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToSignaturePayload;

	impl<Call: Dispatchable> TransactionExtension<Call> for AddToSignaturePayload {
		const IDENTIFIER: &'static str = "AddToSignaturePayload";
		type Implicit = u32;

		fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
			Ok(1234)
		}
		type Pre = ();
		type Val = ();

		impl_tx_ext_default!(Call; weight validate prepare);
	}
}
