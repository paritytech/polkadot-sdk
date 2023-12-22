// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: LGPL-3.0-only

package sr25519

import (
	"crypto/rand"

	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/snowfork/go-substrate-rpc-client/v4/signature"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/crypto"
)

var _ crypto.Keypair = &Keypair{}

type Keypair struct {
	keyringPair *signature.KeyringPair
}

func GenerateKeypair(network uint8) (*Keypair, error) {
	data := make([]byte, 32)
	_, err := rand.Read(data)
	if err != nil {
		return nil, err
	}
	return NewKeypairFromSeed("//"+hexutil.Encode(data), network)
}

func NewKeypairFromSeed(seed string, network uint8) (*Keypair, error) {
	kp, err := signature.KeyringPairFromSecret(seed, network)
	return &Keypair{&kp}, err
}

func NewKeypairFromKRP(pair signature.KeyringPair) *Keypair {
	return &Keypair{&pair}
}

// AsKeyringPair returns the underlying KeyringPair
func (kp *Keypair) AsKeyringPair() *signature.KeyringPair {
	return kp.keyringPair
}

// Encode uses scale to encode underlying KeyringPair
func (kp *Keypair) Encode() []byte {
	out, _ := types.EncodeToBytes(kp.keyringPair)
	return out
}

// Decode initializes keypair by decoding input as a KeyringPair
func (kp *Keypair) Decode(in []byte) error {
	kp.keyringPair = &signature.KeyringPair{}
	return types.DecodeFromBytes(in, kp.keyringPair)
}

// Address returns the ss58 formatted address
func (kp *Keypair) Address() string {
	return kp.keyringPair.Address
}

// PublicKey returns the publickey encoded as a string
func (kp *Keypair) PublicKey() string {
	return hexutil.Encode(kp.keyringPair.PublicKey)
}
