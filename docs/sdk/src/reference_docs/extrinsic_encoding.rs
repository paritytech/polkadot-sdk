//! # Constructing and Signing Extrinsics
//!
//! Extrinsics are payloads that are stored in blocks which are responsible for altering the state
//! of a blockchain via the [_state transition
//! function_][crate::reference_docs::blockchain_state_machines].
//!
//! Substrate is configurable enough that extrinsics can take any format. In practice, runtimes
//! tend to use our [`sp_runtime::generic::UncheckedExtrinsic`] type to represent extrinsics,
//! because it's generic enough to cater for most (if not all) use cases. In Polkadot, this is
//! configured [here](https://github.com/polkadot-fellows/runtimes/blob/94b2798b69ba6779764e20a50f056e48db78ebef/relay/polkadot/src/lib.rs#L1478)
//! at the time of writing.
//!
//! What follows is a description of how extrinsics based on this
//! [`sp_runtime::generic::UncheckedExtrinsic`] type are encoded into bytes. Specifically, we are
//! looking at how extrinsics with a format version of 4 are encoded. This version is itself a part
//! of the payload, and if it changes, it indicates that something about the encoding may have
//! changed.
//!
//! # Encoding an Extrinsic
//!
//! At a high level, all extrinsics compatible with [`sp_runtime::generic::UncheckedExtrinsic`]
//! are formed from concatenating some details together, as in the following pseudo-code:
//!
//! ```text
//! extrinsic_bytes = concat(
//!     compact_encoded_length,
//!     version_and_maybe_signature,
//!     call_data
//! )
//! ```
//!
//! For clarity, the actual implementation in Substrate looks like this:
#![doc = docify::embed!("../../substrate/primitives/runtime/src/generic/unchecked_extrinsic.rs", unchecked_extrinsic_encode_impl)]
//!
//! Let's look at how each of these details is constructed:
//!
//! ## compact_encoded_length
//!
//! This is a [SCALE compact encoded][frame::deps::codec::Compact] integer which is equal to the
//! length, in bytes, of the rest of the extrinsic details.
//!
//! To obtain this value, we must encode and concatenate together the rest of the extrinsic details
//! first, and then obtain the byte length of these. We can then compact encode that length, and
//! prepend it to the rest of the details.
//!
//! ## version_and_maybe_signature
//!
//! If the extrinsic is _unsigned_, then `version_and_maybe_signature` will be just one byte
//! denoting the _transaction protocol version_, which is 4 (or `0b0000_0100`).
//!
//! If the extrinsic is _signed_ (all extrinsics submitted from users must be signed), then
//! `version_and_maybe_signature` is obtained by concatenating some details together, ie:
//!
//! ```text
//! version_and_maybe_signature = concat(
//!     version_and_signed,
//!     from_address,
//!     signature,
//!     signed_extensions_extra,
//! )
//! ```
//!
//! Each of the details to be concatenated together is explained below:
//!
//! ### version_and_signed
//!
//! This is one byte, equal to `0x84` or `0b1000_0100` (i.e. an upper 1 bit to denote that it is
//! signed, and then the transaction version, 4, in the lower bits).
//!
//! ### from_address
//!
//! This is the [SCALE encoded][frame::deps::codec] address of the sender of the extrinsic. The
//! address is the first generic parameter of [`sp_runtime::generic::UncheckedExtrinsic`], and so
//! can vary from chain to chain.
//!
//! The address type used on the Polkadot relay chain is [`sp_runtime::MultiAddress<AccountId32>`],
//! where `AccountId32` is defined [here][`sp_core::crypto::AccountId32`]. When constructing a
//! signed extrinsic to be submitted to a Polkadot node, you'll always use the
//! [`sp_runtime::MultiAddress::Id`] variant to wrap your `AccountId32`.
//!
//! ### signature
//!
//! This is the [SCALE encoded][frame::deps::codec] signature. The signature type is configured via
//! the third generic parameter of [`sp_runtime::generic::UncheckedExtrinsic`], which determines the
//! shape of the signature and signing algorithm that should be used.
//!
//! The signature is obtained by signing the _signed payload_ bytes (see below on how this is
//! constructed) using the private key associated with the address and correct algorithm.
//!
//! The signature type used on the Polkadot relay chain is [`sp_runtime::MultiSignature`]; the
//! variants there are the types of signature that can be provided.
//!
//! ### signed_extensions_extra
//!
//! This is the concatenation of the [SCALE encoded][frame::deps::codec] bytes representing each of
//! the [_signed extensions_][sp_runtime::traits::SignedExtension], and are configured by the
//! fourth generic parameter of [`sp_runtime::generic::UncheckedExtrinsic`]. Learn more about
//! signed extensions [here][crate::reference_docs::signed_extensions].
//!
//! When it comes to constructing an extrinsic, each signed extension has two things that we are
//! interested in here:
//!
//! - The actual SCALE encoding of the signed extension type itself; this is what will form our
//!   `signed_extensions_extra` bytes.
//! - An `AdditionalSigned` type. This is SCALE encoded into the `signed_extensions_additional` data
//!   of the _signed payload_ (see below).
//!
//! Either (or both) of these can encode to zero bytes.
//!
//! Each chain configures the set of signed extensions that it uses in its runtime configuration.
//! At the time of writing, Polkadot configures them
//! [here](https://github.com/polkadot-fellows/runtimes/blob/1dc04eb954eadf8aadb5d83990b89662dbb5a074/relay/polkadot/src/lib.rs#L1432C25-L1432C25).
//! Some of the common signed extensions are defined
//! [here][frame::deps::frame_system#signed-extensions].
//!
//! Information about exactly which signed extensions are present on a chain and in what order is
//! also a part of the metadata for the chain. For V15 metadata, it can be
//! [found here][frame::deps::frame_support::__private::metadata::v15::ExtrinsicMetadata].
//!
//! ## call_data
//!
//! This is the main payload of the extrinsic, which is used to determine how the chain's state is
//! altered. This is defined by the second generic parameter of
//! [`sp_runtime::generic::UncheckedExtrinsic`].
//!
//! A call can be anything that implements [`Encode`][frame::deps::codec::Encode]. In FRAME-based
//! runtimes, a call is represented as an enum of enums, where the outer enum represents the FRAME
//! pallet being called, and the inner enum represents the call being made within that pallet, and
//! any arguments to it. Read more about the call enum
//! [here][crate::reference_docs::frame_runtime_types].
//!
//! FRAME `Call` enums are automatically generated, and end up looking something like this:
#![doc = docify::embed!("./src/reference_docs/extrinsic_encoding.rs", call_data)]
//!
//! In pseudo-code, this `Call` enum encodes equivalently to:
//!
//! ```text
//! call_data = concat(
//!     pallet_index,
//!     call_index,
//!     call_args
//! )
//! ```
//!
//! - `pallet_index` is a single byte denoting the index of the pallet that we are calling into, and
//!   is what the tag of the outermost enum will encode to.
//! - `call_index` is a single byte denoting the index of the call that we are making the pallet,
//!   and is what the tag of the inner enum will encode to.
//! - `call_args` are the SCALE encoded bytes for each of the arguments that the call expects, and
//!   are typically provided as values to the inner enum.
//!
//! Information about the pallets that exist for a chain (including their indexes), the calls
//! available in each pallet (including their indexes), and the arguments required for each call
//! can be found in the metadata for the chain. For V15 metadata, this information
//! [is here][frame::deps::frame_support::__private::metadata::v15::PalletMetadata].
//!
//! # The Signed Payload Format
//!
//! All extrinsics submitted to a node from the outside world (also known as _transactions_) need to
//! be _signed_. The data that needs to be signed for some extrinsic is called the _signed payload_,
//! and its shape is described by the following pseudo-code:
//!
//! ```text
//! signed_payload = concat(
//!     call_data,
//!     signed_extensions_extra,
//!     signed_extensions_additional,
//! )
//!
//! if length(signed_payload) > 256 {
//!     signed_payload = blake2_256(signed_payload)
//! }
//! ```
//!
//! The bytes representing `call_data` and `signed_extensions_extra` can be obtained as described
//! above. `signed_extensions_additional` is constructed by SCALE encoding the
//! ["additional signed" data][sp_runtime::traits::SignedExtension::AdditionalSigned] for each
//! signed extension that the chain is using, in order.
//!
//! Once we've concatenated those together, we hash the result if it's greater than 256 bytes in
//! length using a Blake2 256bit hasher.
//!
//! The [`sp_runtime::generic::SignedPayload`] type takes care of assembling the correct payload
//! for us, given `call_data` and a tuple of signed extensions.
//!
//! # Example Encoding
//!
//! Using [`sp_runtime::generic::UncheckedExtrinsic`], we can construct and encode an extrinsic
//! as follows:
#![doc = docify::embed!("./src/reference_docs/extrinsic_encoding.rs", encoding_example)]

#[docify::export]
pub mod call_data {
	use parity_scale_codec::{Decode, Encode};

	// The outer enum composes calls within
	// different pallets together. We have two
	// pallets, "PalletA" and "PalletB".
	#[derive(Encode, Decode)]
	pub enum Call {
		#[codec(index = 0)]
		PalletA(PalletACall),
		#[codec(index = 7)]
		PalletB(PalletBCall),
	}

	// An inner enum represents the calls within
	// a specific pallet. "PalletA" has one call,
	// "Foo".
	#[derive(Encode, Decode)]
	pub enum PalletACall {
		#[codec(index = 0)]
		Foo(String),
	}

	#[derive(Encode, Decode)]
	pub enum PalletBCall {
		#[codec(index = 0)]
		Bar(String),
	}
}

#[docify::export]
pub mod encoding_example {
	use super::call_data::{Call, PalletACall};
	use crate::reference_docs::signed_extensions::signed_extensions_example;
	use parity_scale_codec::Encode;
	use sp_core::crypto::AccountId32;
	use sp_keyring::sr25519::Keyring;
	use sp_runtime::{
		generic::{SignedPayload, UncheckedExtrinsic},
		MultiAddress, MultiSignature,
	};

	// Define some signed extensions to use. We'll use a couple of examples
	// from the signed extensions reference doc.
	type SignedExtensions =
		(signed_extensions_example::AddToPayload, signed_extensions_example::AddToSignaturePayload);

	// We'll use `UncheckedExtrinsic` to encode our extrinsic for us. We set
	// the address and signature type to those used on Polkadot, use our custom
	// `Call` type, and use our custom set of `SignedExtensions`.
	type Extrinsic =
		UncheckedExtrinsic<MultiAddress<AccountId32, ()>, Call, MultiSignature, SignedExtensions>;

	pub fn encode_demo_extrinsic() -> Vec<u8> {
		// The "from" address will be our Alice dev account.
		let from_address = MultiAddress::<AccountId32, ()>::Id(Keyring::Alice.to_account_id());

		// We provide some values for our expected signed extensions.
		let signed_extensions = (
			signed_extensions_example::AddToPayload(1),
			signed_extensions_example::AddToSignaturePayload,
		);

		// Construct our call data:
		let call_data = Call::PalletA(PalletACall::Foo("Hello".to_string()));

		// The signed payload. This takes care of encoding the call_data,
		// signed_extensions_extra and signed_extensions_additional, and hashing
		// the result if it's > 256 bytes:
		let signed_payload = SignedPayload::new(&call_data, signed_extensions.clone());

		// Sign the signed payload with our Alice dev account's private key,
		// and wrap the signature into the expected type:
		let signature = {
			let sig = Keyring::Alice.sign(&signed_payload.encode());
			MultiSignature::Sr25519(sig)
		};

		// Now, we can build and encode our extrinsic:
		let ext = Extrinsic::new_signed(call_data, from_address, signature, signed_extensions);

		let encoded_ext = ext.encode();
		encoded_ext
	}
}
