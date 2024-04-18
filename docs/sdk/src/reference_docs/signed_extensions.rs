//! Signed extensions are, briefly, a means for different chains to extend the "basic" extrinsic
//! format with custom data that can be checked by the runtime.
//!
//! # Example
//!
//! Defining a couple of very simple signed extensions looks like the following:
#![doc = docify::embed!("./src/reference_docs/signed_extensions.rs", signed_extensions_example)]

#[docify::export]
pub mod signed_extensions_example {
	use parity_scale_codec::{Decode, Encode};
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
