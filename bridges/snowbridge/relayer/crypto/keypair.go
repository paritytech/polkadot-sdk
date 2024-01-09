// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: LGPL-3.0-only

/*
Package crypto is used to provide functionality to several keypair types.
The current supported types are secp256k1 and sr25519.

# Keypairs

The keypair interface is used to bridge the different types of crypto formats.
Every Keypair has both a Encode and Decode function that allows writing and reading from keystore files.
There is also the Address and PublicKey functions that allow access to public facing fields.

# Types

A general overview on the secp256k1 can be found here: https://en.bitcoin.it/wiki/Secp256k1
A general overview on the sr25519 type can be found here: https://wiki.polkadot.network/docs/en/learn-cryptography
*/
package crypto

type KeyType = string

const Sr25519Type KeyType = "sr25519"
const Secp256k1Type KeyType = "secp256k1"

type Keypair interface {
	// Encode is used to write the key to a file
	Encode() []byte
	// Decode is used to retrieve a key from a file
	Decode([]byte) error
	// Address provides the address for the keypair
	Address() string
	// PublicKey returns the keypair's public key an encoded a string
	PublicKey() string
}
