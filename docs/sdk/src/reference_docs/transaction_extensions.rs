//! Transaction extensions are, briefly, a means for different chains to extend the "basic"
//!
//!
//!
//!   network. Determined based on genesis.
//!
//!   mortality.
//!
//!   transaction is not the *all zero account* (all bytes of the accountid are zero).
//!
//!   of transactions and to provide ordering of transactions.
//!
//!   the currently active runtime.
//!
//!   correct encoding of the call.
//!
//!   before dispatching it.
//!
//!   transaction fees from the signer based on the weight of the call using the native token.
//!
//!   fees from the signer based on the weight of the call using any supported asset (including the
//!
//!   conversion)`]: Charges transaction
//!   native token). The asset is converted to the native token using a pool.
//!
//!   to be processed without paying any fee. This requires that the `call` that should be
//!   attribute.
//!
//!   to include the so-called metadata hash. This is required by chains to support the generic
//!
//!   transaction extension for parachains that reclaims unused storage weight after executing a
//!
//!
//!
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

// [`ChargeAssetTxPayment`]: pallet_asset_tx_payment::ChargeAssetTxPayment
// [`ChargeAssetTxPayment`(using
//!   conversion)`]: using
//!   conversion)](pallet_asset_conversion_tx_payment::ChargeAssetTxPayment

// [`feeless_if`]: frame_support::pallet_macros::feeless_if
