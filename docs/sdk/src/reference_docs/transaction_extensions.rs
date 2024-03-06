//! Transaction extensions are, briefly, a means for different chains to extend the "basic"
//! extrinsic format with custom data that can be checked by the runtime.
//!
//! # Example
//!
//! Defining a couple of very simple transaction extensions looks like the following:
#![doc = docify::embed!("./src/reference_docs/transaction_extensions.rs", transaction_extensions_example)]

#[docify::export]
pub mod transaction_extensions_example {
	use parity_scale_codec::{Decode, Encode};
	use scale_info::TypeInfo;
	use sp_runtime::{
		impl_tx_ext_default,
		traits::{Dispatchable, TransactionExtension, TransactionExtensionBase},
		TransactionValidityError,
	};

	// This doesn't actually check anything, but simply allows
	// some arbitrary `u32` to be added to the extrinsic payload
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToPayload(pub u32);

	impl TransactionExtensionBase for AddToPayload {
		const IDENTIFIER: &'static str = "AddToPayload";
		type Implicit = ();
	}

	impl<Call: Dispatchable> TransactionExtension<Call, ()> for AddToPayload {
		type Pre = ();
		type Val = ();

		impl_tx_ext_default!(Call; (); validate prepare);
	}

	// This is the opposite; nothing will be added to the extrinsic payload,
	// but the Implicit type (`1234u32`) will be added to the
	// payload to be signed.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToSignaturePayload;

	impl TransactionExtensionBase for AddToSignaturePayload {
		const IDENTIFIER: &'static str = "AddToSignaturePayload";
		type Implicit = u32;

		fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
			Ok(1234)
		}
	}

	impl<Call: Dispatchable> TransactionExtension<Call, ()> for AddToSignaturePayload {
		type Pre = ();
		type Val = ();

		impl_tx_ext_default!(Call; (); validate prepare);
	}
}
