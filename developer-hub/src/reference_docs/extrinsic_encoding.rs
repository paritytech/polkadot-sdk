//! # Constructing and Signing Extrinsics
//!
//! Extrinsics are payloads that are stored in blocks which are responsible for altering the state
//! of a blockchain via the [_state transition function_][crate::reference_docs::blockchain_state_machines].
//!
//! Substrate is configurable enough that extrinsics can take any format. In practice, runtimes
//! tend to use our [`sp_runtime::generic::UncheckedExtrinsic`] type to represent extrinsics,
//! because it's generic enough to cater for most (if not all) use cases. In Polkadot, this is configured
//! [here](https://github.com/polkadot-fellows/runtimes/blob/94b2798b69ba6779764e20a50f056e48db78ebef/relay/polkadot/src/lib.rs#L1478)
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
//! For clarity, the actual implementation
//! [is here](https://github.com/paritytech/polkadot-sdk/blob/35eb133baab93d3f2f179df216b2cc175d7dcaf2/substrate/primitives/runtime/src/generic/unchecked_extrinsic.rs#L296)
//! at the time of writing.
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
//! denoting the _transaction protocol version_, which is 4.
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
//! This is one byte, equal to `0x84` or `0b1000_0100` (ie an upper 1 bit to denote that it is
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
//! [`sp_runtime::MultiAddress::Id`] variant.
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
//! fourth generic parameter of [`sp_runtime::generic::UncheckedExtrinsic`]. Signed extensions are,
//! briefly, a means for different chains to extend the "basic" extrinsic format with custom data
//! that can be checked by the runtime.
//!
//! When it comes to constructing an extrinsic, each signed extension has two things that we are
//! interested in here:
//!
//! - The actual SCALE encoding of the signed extension type itself; this is what will form our
//!   `signed_extensions_extra` bytes.
//! - An `AdditionalSigned` type. This is SCALE encoded into the `signed_extensions_additional`
//!   data of the _signed payload_ (see below).
//!
//! Either (or both) of these can encode to zero bytes.
//!
//! Each chain configures the set of signed extensions that it uses in its runtime configuration.
//! At the time of writing, Polkadot configures them
//! [here](https://github.com/polkadot-fellows/runtimes/blob/1dc04eb954eadf8aadb5d83990b89662dbb5a074/relay/polkadot/src/lib.rs#L1432C25-L1432C25).
//! Some of the common signed extensions are defined [here][frame::deps::frame_system].
//!
//! Information about exactly which signed extensions are present on a chain and in what order is
//! also a part of the metadata for the chain. For V15 metadata, it can be
//! [found here][frame::deps::frame_support::__private::metadata::v15::ExtrinsicMetadata].
//!
//! ## call_data
//!
//! This data defines exactly which call is made by the extrinsic, and with what arguments. These
//! are determined by the second generic parameter of [`sp_runtime::generic::UncheckedExtrinsic`].
//!
//! In pseudo-code, a call looks like this:
//!
//! ```text
//! call_data = concat(
//!     pallet_index,
//!     call_index,
//!     call_args
//! )
//! ```
//!
//! - `pallet_index` is a single byte denoting the index of the pallet that we are calling into.
//! - `call_index` is a single byte denoting the index of the call that we are making the pallet.
//! - `call_args` are the SCALE encoded bytes for each of the arguments that the call expects.
//!
//! Information about the pallets that exist for a chain (including their indexes), the calls
//! available in each pallet (including their indexes), and the arguments required for each call
//! can be found in the metadata for the chain. For V15 metadata, this information
//! [is here][frame::deps::frame_support::__private::metadata::v15::PalletMetadata].
//!
//! # The Signed Payload Format
//!
//! All extrinsics submitted to a node from the outside world (also known as _transactions_) need to
//! be _signed_. The data that needs to be signed for some extrinsic is called the _signed payload_
//! (also called the _signed payload_), and its shape is described by the following pseudo-code:
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
//! In other words, we create the signed payload by concatenating the bytes representing
//! `call_data`, `signed_extensions_extra` and `signed_extensions_additional` together. If this
//! payload is more than 256 bytes in size, we hash it using a 256bit Blake2 hasher.
//!
//! How to construct the `call_data` and `signed_extensions_extra` has already been explained above.
//! `signed_extensions_additional` is constructed by SCALE encoding the
//! ["additional signed" data][sp_runtime::traits::SignedExtension::AdditionalSigned] for each signed
//! extension that the chain is using, in order, and concatenating the resulting bytes together.
//!
//! If the resulting bytes have a length greater than 256, then we hash them using a Blake2 256bit
//! hasher, and that hash is the signed payload.
