// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package parachain

import (
	"fmt"

	gethCommon "github.com/ethereum/go-ethereum/common"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
)

type EventLog struct {
	Address types.H160
	Topics  []types.H256
	Data    types.Bytes
}

type Message struct {
	EventLog EventLog
	Proof    Proof
}

type Proof struct {
	BlockHash types.H256
	TxIndex   types.U32
	Data      *ProofData
}

type ProofData struct {
	Keys   []types.Bytes
	Values []types.Bytes
}

func NewProofData() *ProofData {
	return &ProofData{
		Keys:   make([]types.Bytes, 0),
		Values: make([]types.Bytes, 0),
	}
}

// For interface ethdb.KeyValueWriter
func (p *ProofData) Put(key []byte, value []byte) error {
	p.Keys = append(p.Keys, types.NewBytes(gethCommon.CopyBytes(key)))
	p.Values = append(p.Values, types.NewBytes(gethCommon.CopyBytes(value)))
	return nil
}

// For interface ethdb.KeyValueWriter
func (p *ProofData) Delete(_ []byte) error {
	return fmt.Errorf("Delete should never be called to generate a proof")
}
