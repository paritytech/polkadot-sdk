//! # Constructing and Signing Extrinsics
//!
//! of a blockchain via the [_state transition
//!
//! tend to use our [`UncheckedExtrinsic`] type to represent extrinsics,
//! configured [`here`]
//!
//! [`UncheckedExtrinsic`] type are encoded into bytes. Specifically, we are
//! of the payload, and if it changes, it indicates that something about the encoding may have
//!
//!
//! are formed from concatenating some details together, as in the following pseudo-code:
//!
//! extrinsic_bytes = concat(
//!     version_and_extrinsic_type,
//!     call_data
//! ```
//!
#![doc = docify::embed!("../../substrate/primitives/runtime/src/generic/unchecked_extrinsic.rs", unchecked_extrinsic_encode_impl)]
//!
//!
//!
//! length, in bytes, of the rest of the extrinsic details.
//!
//! first, and then obtain the byte length of these. We can then compact encode that length, and
//!
//!
//! denoting the _transaction protocol version_, which is 4 (or `0b0000_0100`).
//!
//! `version_and_maybe_signature` is obtained by concatenating some details together, ie:
//!
//! version_and_maybe_signature = concat(
//!     from_address,
//!     transaction_extensions_extra,
//! ```
//!
//!
//!
//! - the 2 most significant bits represent the extrinsic type:
//!     - signed - `0b10`
//! - the 6 least significant bits represent the extrinsic format version (currently 5)
//!
//!
//! protocol version_, which is 5 (or `0b0000_0101`). Bare extrinsics do not carry any other
//! `version_and_extrinsic_type` would always be followed by the encoded call bytes.
//!
//!
//! version 4), then `version_and_extrinsic_type` is obtained by having a MSB of `1` on the
//!
//! as follows:
//!
//!
//! address is the first generic parameter of [`UncheckedExtrinsic`], and so
//!
//! where `AccountId32` is defined [here][`sp_core::crypto::AccountId32`]. When constructing a
//! [`Id`] variant to wrap your `AccountId32`.
//!
//!
//! the third generic parameter of [`UncheckedExtrinsic`], which determines the
//!
//! constructed) using the private key associated with the address and correct algorithm.
//!
//! variants there are the types of signature that can be provided.
//!
//!
//! data), then `version_and_extrinsic_type` is obtained by logical OR between the general
//! `0b0100_0101`).
//!
//!
//! single byte describing the extension version (this is bumped whenever a change occurs in the
//! extensions_][sp_runtime::traits::TransactionExtension], and are configured by the fourth generic
//! extensions [here][crate::reference_docs::transaction_extensions].
//!
//! are interested in here:
//!
//!   `transaction_extensions_extra` bytes.
//!   below).
//!
//!
//! configuration. At the time of writing, Polkadot configures them
//! Some of the common transaction extensions are defined
//!
//! is also a part of the metadata for the chain. For V15 metadata, it can be [found
//!
//!
//! altered. This is defined by the second generic parameter of
//!
//! runtimes, a call is represented as an enum of enums, where the outer enum represents the FRAME
//! any arguments to it. Read more about the call enum
//!
#![doc = docify::embed!("./src/reference_docs/extrinsic_encoding.rs", call_data)]
//!
//!
//! call_data = concat(
//!     call_index,
//! )
//!
//!   is what the tag of the outermost enum will encode to.
//!   and is what the tag of the inner enum will encode to.
//!   are typically provided as values to the inner enum.
//!
//! available in each pallet (including their indexes), and the arguments required for each call can
//! here][frame::deps::frame_support::__private::metadata::v15::PalletMetadata].
//!
//!
//! _transactions_) need to be _signed_. The data that needs to be signed for some extrinsic is
//!
//! signed_payload = blake2_256(
//!     	call_data,
//!     	transaction_extensions_implicit,
//! )
//!
//! descibed above. `transaction_extensions_implicit` is constructed by SCALE encoding the
//! extension that the chain is using, in order.
//!
//!
//! us, given `call_data` and a tuple of transaction extensions.
//!
//!
//! extrinsic, such as a traditionally signed transaction. Instead, general transactions should have
//! one of which could be the traditional signature check that happens for all signed transactions
//! [`UncheckedExtrinsic`]. Therefore, it is up to each
//! origin type. For an example, look into the [`authorization example pallet
//!
//!
//! follows:
#![doc = docify::embed!("./src/reference_docs/extrinsic_encoding.rs", encoding_example)]

#[docify::export]
pub mod call_data {
	use codec::{Decode, Encode};
	use sp_runtime::{traits::Dispatchable, DispatchResultWithInfo};

	// The outer enum composes calls within
	// different pallets together. We have two
	// pallets, "PalletA" and "PalletB".
	#[derive(Encode, Decode, Clone)]
	pub enum Call {
		#[codec(index = 0)]
		PalletA(PalletACall),
		#[codec(index = 7)]
		PalletB(PalletBCall),
	}

	// An inner enum represents the calls within
	// a specific pallet. "PalletA" has one call,
	// "Foo".
	#[derive(Encode, Decode, Clone)]
	pub enum PalletACall {
		#[codec(index = 0)]
		Foo(String),
	}

	#[derive(Encode, Decode, Clone)]
	pub enum PalletBCall {
		#[codec(index = 0)]
		Bar(String),
	}

	impl Dispatchable for Call {
		type RuntimeOrigin = ();
		type Config = ();
		type Info = ();
		type PostInfo = ();
		fn dispatch(self, _origin: Self::RuntimeOrigin) -> DispatchResultWithInfo<Self::PostInfo> {
			Ok(())
		}
	}
}

#[docify::export]
pub mod encoding_example {
	use super::call_data::{Call, PalletACall};
	use crate::reference_docs::transaction_extensions::transaction_extensions_example;
	use codec::Encode;
	use sp_core::crypto::AccountId32;
	use sp_keyring::sr25519::Keyring;
	use sp_runtime::{
		generic::{SignedPayload, UncheckedExtrinsic},
		MultiAddress, MultiSignature,
	};

	// Define some transaction extensions to use. We'll use a couple of examples
	// from the transaction extensions reference doc.
	type TransactionExtensions = (
		transaction_extensions_example::AddToPayload,
		transaction_extensions_example::AddToSignaturePayload,
	);

	// We'll use `UncheckedExtrinsic` to encode our extrinsic for us. We set
	// the address and signature type to those used on Polkadot, use our custom
	// `Call` type, and use our custom set of `TransactionExtensions`.
	type Extrinsic = UncheckedExtrinsic<
		MultiAddress<AccountId32, ()>,
		Call,
		MultiSignature,
		TransactionExtensions,
	>;

	pub fn encode_demo_extrinsic() -> Vec<u8> {
		// The "from" address will be our Alice dev account.
		let from_address = MultiAddress::<AccountId32, ()>::Id(Keyring::Alice.to_account_id());

		// We provide some values for our expected transaction extensions.
		let transaction_extensions = (
			transaction_extensions_example::AddToPayload(1),
			transaction_extensions_example::AddToSignaturePayload,
		);

		// Construct our call data:
		let call_data = Call::PalletA(PalletACall::Foo("Hello".to_string()));

		// The signed payload. This takes care of encoding the call_data,
		// transaction_extensions_extra and transaction_extensions_implicit, and hashing
		// the result if it's > 256 bytes:
		let signed_payload = SignedPayload::new(call_data.clone(), transaction_extensions.clone());

		// Sign the signed payload with our Alice dev account's private key,
		// and wrap the signature into the expected type:
		let signature = {
			let sig = Keyring::Alice.sign(&signed_payload.encode());
			MultiSignature::Sr25519(sig)
		};

		// Now, we can build and encode our extrinsic:
		let ext = Extrinsic::new_signed(call_data, from_address, signature, transaction_extensions);

		let encoded_ext = ext.encode();
		encoded_ext
	}
}

// [`MultiAddress<AccountId32>`]: sp_runtime::MultiAddress<AccountId32>
// [`SignedPayload`]: sp_runtime::generic::SignedPayload

// [`MultiAddress<AccountId32>`]: sp_runtime::MultiAddress<AccountId32>
// [`SignedPayload`]: sp_runtime::generic::SignedPayload

// [`MultiAddress<AccountId32>`]: sp_runtime::MultiAddress<AccountId32>
// [`SignedPayload`]: sp_runtime::generic::SignedPayload

// [`MultiAddress<AccountId32>`]: sp_runtime::MultiAddress<AccountId32>
// [`SignedPayload`]: sp_runtime::generic::SignedPayload

// [`MultiAddress<AccountId32>`]: sp_runtime::MultiAddress<AccountId32>
// [`SignedPayload`]: sp_runtime::generic::SignedPayload

// [`Checkable`]: sp_runtime::traits::Checkable
// [`UncheckedExtrinsic`]: sp_runtime::generic::UncheckedExtrinsic
// [`authorization example pallet
//! extensions`]: pallet_example_authorization_tx_extension::extensions
// [`here`]: https://github.com/polkadot-fellows/runtimes/blob/1dc04eb954eadf8aadb5d83990b89662dbb5a074/relay/polkadot/src/lib.rs#L1432C25-L1432C25
