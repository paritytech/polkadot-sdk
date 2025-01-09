// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Crypto utilities for testing and benchmarking in crowdloan pallet.

#[cfg(any(feature = "runtime-benchmarks", test))]
use alloc::vec::Vec;
use sp_core::ed25519;
use sp_io::crypto::{ed25519_generate, ed25519_sign};
use sp_runtime::{MultiSignature, MultiSigner};

pub fn create_ed25519_pubkey(seed: Vec<u8>) -> MultiSigner {
	ed25519_generate(0.into(), Some(seed)).into()
}

pub fn create_ed25519_signature(payload: &[u8], pubkey: MultiSigner) -> MultiSignature {
	let edpubkey = ed25519::Public::try_from(pubkey).unwrap();
	let edsig = ed25519_sign(0.into(), &edpubkey, payload).unwrap();
	edsig.into()
}
