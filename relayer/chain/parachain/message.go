// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package parachain

import (
	"fmt"
	"strings"

	gethCommon "github.com/ethereum/go-ethereum/common"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/relays/util"
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

type MessageJSON struct {
	EventLog EventLogJSON `json:"event_log"`
	Proof    ProofJSON
}

type EventLogJSON struct {
	Address string   `json:"address"`
	Topics  []string `json:"topics"`
	Data    string   `json:"data"`
}

type ProofJSON struct {
	BlockHash string         `json:"block_hash"`
	TxIndex   uint32         `json:"tx_index"`
	Data      *ProofDataJSON `json:"data"`
}

type ProofDataJSON struct {
	Keys   []string `json:"keys"`
	Values []string `json:"values"`
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

func (m Message) ToJSON() MessageJSON {
	return MessageJSON{
		EventLog: EventLogJSON{
			Address: m.EventLog.Address.Hex(),
			Topics:  util.ScaleBranchToString(m.EventLog.Topics),
			Data:    m.EventLog.Data.Hex(),
		},
		Proof: ProofJSON{
			BlockHash: m.Proof.BlockHash.Hex(),
			TxIndex:   uint32(m.Proof.TxIndex),
			Data: &ProofDataJSON{
				Keys:   util.ScaleBytesToArrayHexArray(m.Proof.Data.Keys),
				Values: util.ScaleBytesToArrayHexArray(m.Proof.Data.Values),
			},
		},
	}
}

func (m *MessageJSON) RemoveLeadingZeroHashes() {
	m.EventLog.RemoveLeadingZeroHashes()
	m.Proof.RemoveLeadingZeroHashes()
}

func (e *EventLogJSON) RemoveLeadingZeroHashes() {
	e.Address = removeLeadingZeroHash(e.Address)
	e.Topics = removeLeadingZeroHashForSlice(e.Topics)
	e.Data = removeLeadingZeroHash(e.Data)
}

func (p *ProofJSON) RemoveLeadingZeroHashes() {
	p.BlockHash = removeLeadingZeroHash(p.BlockHash)
	p.Data.RemoveLeadingZeroHashes()
}

func (p *ProofDataJSON) RemoveLeadingZeroHashes() {
	p.Keys = removeLeadingZeroHashForSlice(p.Keys)
	p.Values = removeLeadingZeroHashForSlice(p.Values)
}

func removeLeadingZeroHashForSlice(s []string) []string {
	result := make([]string, len(s))

	for i, item := range s {
		result[i] = removeLeadingZeroHash(item)
	}
	return result
}

func removeLeadingZeroHash(s string) string {
	return strings.Replace(s, "0x", "", 1)
}
