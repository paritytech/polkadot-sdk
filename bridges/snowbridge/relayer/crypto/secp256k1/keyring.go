// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package secp256k1

// Keypairs for use in tests

func Alice() *Keypair {
	bz := padWithZeros([]byte("Alice"), PrivateKeyLength)
	kp, err := NewKeypairFromPrivateKey(bz)
	if err != nil {
		panic(err)
	}
	return kp
}

func Bob() *Keypair {
	bz := padWithZeros([]byte("Bob"), PrivateKeyLength)
	kp, err := NewKeypairFromPrivateKey(bz)
	if err != nil {
		panic(err)
	}
	return kp
}

// padWithZeros adds on extra 0 bytes to make a byte array of a specified length
func padWithZeros(key []byte, targetLength int) []byte {
	res := make([]byte, targetLength-len(key))
	return append(res, key...)
}
