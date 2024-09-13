//! Signed extensions are, briefly, a means for different chains to extend the "basic" extrinsic
//! format with custom data that can be checked by the runtime.
//!
//! # FRAME provided signed extensions
//!
//! FRAME by default already provides the following signed extensions:
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
//!   signed extension for parachains that reclaims unused storage weight after executing a
//!   transaction.
//!
//! For more information about these extensions, follow the link to the type documentation.
//!
//! # Building a custom signed extension
//!
//! Defining a couple of very simple signed extensions looks like the following:
#![doc = docify::embed!("./src/reference_docs/signed_extensions.rs", signed_extensions_example)]

#[docify::export]
pub mod signed_extensions_example {
	use codec::{Decode, Encode};
	use scale_info::TypeInfo;
	use sp_runtime::traits::SignedExtension;

	// This doesn't actually check anything, but simply allows
	// some arbitrary `u32` to be added to the extrinsic payload
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToPayload(pub u32);

	impl SignedExtension for AddToPayload {
		const IDENTIFIER: &'static str = "AddToPayload";
		type AccountId = ();
		type Call = ();
		type AdditionalSigned = ();
		type Pre = ();

		fn additional_signed(
			&self,
		) -> Result<
			Self::AdditionalSigned,
			sp_runtime::transaction_validity::TransactionValidityError,
		> {
			Ok(())
		}

		fn pre_dispatch(
			self,
			_who: &Self::AccountId,
			_call: &Self::Call,
			_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
			_len: usize,
		) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
			Ok(())
		}
	}

	// This is the opposite; nothing will be added to the extrinsic payload,
	// but the AdditionalSigned type (`1234u32`) will be added to the
	// payload to be signed.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToSignaturePayload;

	impl SignedExtension for AddToSignaturePayload {
		const IDENTIFIER: &'static str = "AddToSignaturePayload";
		type AccountId = ();
		type Call = ();
		type AdditionalSigned = u32;
		type Pre = ();

		fn additional_signed(
			&self,
		) -> Result<
			Self::AdditionalSigned,
			sp_runtime::transaction_validity::TransactionValidityError,
		> {
			Ok(1234)
		}

		fn pre_dispatch(
			self,
			_who: &Self::AccountId,
			_call: &Self::Call,
			_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
			_len: usize,
		) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
			Ok(())
		}
	}
}
