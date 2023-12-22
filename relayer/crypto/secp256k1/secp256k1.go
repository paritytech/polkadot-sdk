// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: LGPL-3.0-only

package secp256k1

import (
	"crypto/ecdsa"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/snowfork/snowbridge/relayer/crypto"

	secp256k1 "github.com/ethereum/go-ethereum/crypto"
)

var _ crypto.Keypair = &Keypair{}

const PrivateKeyLength = 32

type Keypair struct {
	public  *ecdsa.PublicKey
	private *ecdsa.PrivateKey
}

func NewKeypairFromPrivateKey(priv []byte) (*Keypair, error) {
	pk, err := secp256k1.ToECDSA(priv)
	if err != nil {
		return nil, err
	}

	return &Keypair{
		public:  pk.Public().(*ecdsa.PublicKey),
		private: pk,
	}, nil
}

// NewKeypairFromString parses a string for a hex private key. Must be at least
// PrivateKeyLength long.
func NewKeypairFromString(priv string) (*Keypair, error) {
	pk, err := secp256k1.HexToECDSA(priv)
	if err != nil {
		return nil, err
	}

	return &Keypair{
		public:  pk.Public().(*ecdsa.PublicKey),
		private: pk,
	}, nil
}

func NewKeypair(pk ecdsa.PrivateKey) *Keypair {
	pub := pk.Public()

	return &Keypair{
		public:  pub.(*ecdsa.PublicKey),
		private: &pk,
	}
}

func GenerateKeypair() (*Keypair, error) {
	priv, err := secp256k1.GenerateKey()
	if err != nil {
		return nil, err
	}

	return NewKeypair(*priv), nil
}

// Encode dumps the private key as bytes
func (kp *Keypair) Encode() []byte {
	return secp256k1.FromECDSA(kp.private)
}

// Decode initializes the keypair using the input
func (kp *Keypair) Decode(in []byte) error {
	key, err := secp256k1.ToECDSA(in)
	if err != nil {
		return err
	}

	kp.public = key.Public().(*ecdsa.PublicKey)
	kp.private = key

	return nil
}

// Address returns the Ethereum address format
func (kp *Keypair) Address() string {
	return secp256k1.PubkeyToAddress(*kp.public).String()
}

// CommonAddress returns the Ethereum address in the common.Address Format
func (kp *Keypair) CommonAddress() common.Address {
	return secp256k1.PubkeyToAddress(*kp.public)
}

// PublicKey returns the public key hex encoded
func (kp *Keypair) PublicKey() string {
	return hexutil.Encode(secp256k1.CompressPubkey(kp.public))
}

// PrivateKey returns the keypair's private key
func (kp *Keypair) PrivateKey() *ecdsa.PrivateKey {
	return kp.private
}
