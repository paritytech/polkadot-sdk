//! # Signed Extensions
//!
//! Signed extensions are a way to extend an extrinsic with additional checks and behaviors.
//! Each chain configures a hardcoded set of signed extensions that are then utilized for every
//! extrinsic that is submitted.
//!
//! Signed extensions [implement the following trait][sp_runtime::traits::SignedExtension]:
#![doc = docify::embed!("../../substrate/primitives/runtime/src/traits.rs", SignedExtension)]
//!
//! Tuples of signed extensions automatically implement this trait too, which allows them to be
//! used in our default [`sp_runtime::generic::UncheckedExtrinsic`].
//!
//! # Working with signed extensions
//!
//! Every signed extension must implement [`parity_scale_codec::Encode`] and
//! [`parity_scale_codec::Decode`] to define how they are to be SCALE encoded and decoded. Every
//! valid extrinsic will then contain all of the encoded signed extension bytes as part of its
//! payload, which the node can then decode. Each signed extension on the node can then perform
//! additional logic for the extrinsic via [`validate()`][validate],
//! [`pre_dispatch()`][pre_dispatch] and [`post_dispatch()`][post_dispatch] methods.
//!
//! Signed extensions can optionally also create some [`AdditionalSigned`][AdditionalSigned] type
//! via [`additional_signed()`][additional_signed]. This type must implement
//! [`parity_scale_codec::Encode`]. The SCALE encoded bytes for all of the additional signed values
//! are added to a _signed payload_, which will be checked against the signature and "from" address
//! of a submitted extrinsic. If the signature in the submitted extrinsic was created from a
//! different signed payload, then the extrinsic is found to be invalid. Thus, the extrinsic sender
//! needs to have the same additional signed data as the node, else it will not be able to produce
//! valid extrinsics.
//!
//! Signed extensions also expose a [`metadata()`][metadata] method whose default implementation
//! should normally be left alone. This determines what information is placed in the node metadata,
//! which in turn informs clients about how to SCALE encode and decode the signed extensions of a
//! node and their additional signed types. Consequently, modifying it could break clients ability
//! to submit and decode extrinsics.
//!
//! For more information on how signed extensions are encoded into extrinsics, see
//! [this page][extrinsic_encoding].
//!
//! # Common signed extensions
//!
//! Let's look at a few common signed extensions, to see how they make use of these properties:
//!
//! ## CheckGenesis
//!
//! [`CheckGenesis`][CheckGenesis] sets its [additional signed][AdditionalSigned] value to be the
//! chain genesis hash. The signed payload of any submitted extrinsic must then contain the same
//! genesis hash, else the signature will be found to be invalid and the extrinsic rejected.
//!
//! Including the genesis hash prevents an extrinsic from being used on any chain other than the
//! intended one. It also protects against a malicious actor submitting the extrinsic on multiple
//! chains.
//!
//! ## CheckSpecVersion
//!
//! [`CheckSpecVersion`][CheckSpecVersion] acts similarly, and places the runtime spec into the
//! signed payload. This causes all extrinsics to become invalid when a runtime update occurs
//! on the chain (which bumps the spec version).
//!
//! This is desirable because a runtime update may change the behavior or format for some calls,
//! which could inadvertently render old extrinsics invalid or change their behavior.
//!
//! ## CheckNonce
//!
//! The [`CheckNonce`][CheckNonce] extension contains a extrinsic index (a nonce) which is
//! expected to equal the nonce currently stored in the on-chain account details for the
//! extrinsic author. If the nonces are equal, then the extrinsic is valid, and the on-chain
//! nonce will be incremented once it is included in a block. If the nonce is smaller than expected,
//! then the extrinsic is likely to always be invalid (unless the account is reaped and re-created,
//! which will start the nonce at 0 again). If the nonce is larger than expected, then the
//! extrinsic may become valid in the future (ie after other extrinsics from the same account are
//! successful in incrementing the nonce up to the required value).
//!
//! Requiring a nonce in extrinsic payloads helps to prevent an extrinsic from being submitted
//! multiple times.
//!
//! ## CheckMortality
//!
//! The [`CheckMortality`][CheckMortality] extension defines how long an extrinsic will be valid
//! for. It can be valid forever (immortal) or valid from some already-seen starting block for a
//! specified power-of-two number of blocks.
//!
//! The extension has two parts to it:
//!
//! - First, it encodes/decodes the _mortality_ of an extrinsic (ie how many blocks will it be valid
//!   for, and the starting block number) into the extrinsic payload. It uses a clever scheme to
//!   encode this information into just 2 bytes.
//! - Second, it uses the starting block number encoded into the mortality data to look up a
//!   corresponding block hash to use as its additional signed payload. This means that if the block
//!   hash found on the node differs from that signed by the extrinsic author (ie because the node
//!   is on a different fork from the extrinsic author) then the extrinsic will be rendered invalid.
//!
//! Several other common signed extensions exist, but these ones are fairly simple and hopefully
//! provide some insight into how signed extensions work.
//!
//! # Example
//!
//! Looking at the code for the signed extensions linked above as well as the
//! [signed extension][SignedExtension] trait docs is a good starting point for writing your own.
//! However, here are a couple of very simple signed extension implementations:
//!
//! [extrinsic_encoding]: crate::reference_docs::extrinsic_encoding
//! [additional_signed]: sp_runtime::traits::SignedExtension::additional_signed()
//! [validate]: sp_runtime::traits::SignedExtension::validate()
//! [pre_dispatch]: sp_runtime::traits::SignedExtension::pre_dispatch()
//! [post_dispatch]: sp_runtime::traits::SignedExtension::post_dispatch()
//! [metadata]: sp_runtime::traits::SignedExtension::metadata()
//! [SignedExtension]: sp_runtime::traits::SignedExtension
//! [AdditionalSigned]: sp_runtime::traits::SignedExtension::AdditionalSigned
//! [CheckGenesis]: frame::deps::frame_system::CheckGenesis
//! [CheckSpecVersion]: frame::deps::frame_system::CheckSpecVersion
//! [CheckTxVersion]: frame::deps::frame_system::CheckTxVersion
//! [CheckNonce]: frame::deps::frame_system::CheckNonce
//! [CheckMortality]: frame::deps::frame_system::CheckMortality
#![doc = docify::embed!("./src/reference_docs/signed_extensions.rs", signed_extensions_example)]

#[docify::export]
pub mod signed_extensions_example {
	use parity_scale_codec::{Decode, Encode};
	use scale_info::TypeInfo;
	use sp_runtime::{
		traits::{DispatchInfoOf, SignedExtension},
		transaction_validity::{TransactionValidity, TransactionValidityError, ValidTransaction},
	};

	// This doesn't actually check anything, but simply allows
	// some arbitrary `u32` to be added to the extrinsic payload
	// and then made use of when validating an extrinsic or putting
	// it into a block.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToPayload(pub u32);

	impl SignedExtension for AddToPayload {
		const IDENTIFIER: &'static str = "AddToPayload";
		type AccountId = ();
		type Call = ();
		type AdditionalSigned = ();
		type Pre = ();

		fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
			Ok(())
		}

		fn validate(
			&self,
			_who: &Self::AccountId,
			_call: &Self::Call,
			_info: &DispatchInfoOf<Self::Call>,
			_len: usize,
		) -> TransactionValidity {
			// We have the option here to mark the transaction valid or invalid
			// based on the u32 payload.

			Ok(ValidTransaction::default())
		}

		fn pre_dispatch(
			self,
			who: &Self::AccountId,
			call: &Self::Call,
			info: &DispatchInfoOf<Self::Call>,
			len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			// Always validate the call in the pre_dispatch function, because
			// the validate function is an optimisation and may not necessarily
			// be called.
			self.validate(who, call, info, len).map(|_| ())?;

			// Now, perform some logic using our u32 payload.

			// The `Self::Pre` type is passed to post_dispatch and provides a means
			// to hand data to that call from this one; here we use a unit type.
			Ok(())
		}
	}

	// This is the opposite; nothing will be added to the extrinsic payload,
	// but the AdditionalSigned type is `1234u32`, which will be added to the
	// payload which will be signed.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	pub struct AddToSignaturePayload;

	impl SignedExtension for AddToSignaturePayload {
		const IDENTIFIER: &'static str = "AddToSignaturePayload";
		type AccountId = ();
		type Call = ();
		type AdditionalSigned = u32;
		type Pre = ();

		fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
			// By doing this, we require extrinsic authors to also add 1234u32 to
			// the signer payloads that they sign when constructing extrinsics,
			// else the signature will be invalid.
			Ok(1234)
		}

		fn pre_dispatch(
			self,
			who: &Self::AccountId,
			call: &Self::Call,
			info: &DispatchInfoOf<Self::Call>,
			len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			self.validate(who, call, info, len).map(|_| ())
		}
	}
}
