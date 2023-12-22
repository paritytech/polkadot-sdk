// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: LGPL-3.0-only

package secp256k1

import (
	"reflect"
	"testing"
)

func TestNewKeypairFromSeed(t *testing.T) {
	kp, err := GenerateKeypair()
	if err != nil {
		t.Fatal(err)
	}

	if kp.PublicKey() == "" || kp.Address() == "" {
		t.Fatalf("key is missing data: %#v", kp)
	}
}

func TestEncodeAndDecodeKeypair(t *testing.T) {
	kp, err := GenerateKeypair()
	if err != nil {
		t.Fatal(err)
	}

	enc := kp.Encode()
	res := new(Keypair)
	err = res.Decode(enc)
	if err != nil {
		t.Fatal(err)
	}

	if !reflect.DeepEqual(res, kp) {
		t.Fatalf("Fail: got %#v expected %#v", res, kp)
	}
}
