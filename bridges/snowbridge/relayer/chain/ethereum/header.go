// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package ethereum

import (
	"fmt"
	etypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/snowfork/go-substrate-rpc-client/v4/scale"
	types "github.com/snowfork/go-substrate-rpc-client/v4/types"
	"math/big"
)

type HeaderID struct {
	Number types.U64
	Hash   types.H256
}

type headerSCALE struct {
	ParentHash       types.H256
	Timestamp        types.U64
	Number           types.U64
	Author           types.H160
	TransactionsRoot types.H256
	OmmersHash       types.H256
	ExtraData        types.Bytes
	StateRoot        types.H256
	ReceiptsRoot     types.H256
	LogsBloom        types.Bytes256
	GasUsed          types.U256
	GasLimit         types.U256
	Difficulty       types.U256
	Seal             []types.Bytes
	BaseFee          optionBaseFee
}

type optionBaseFee struct {
	HasValue bool
	Value    types.U256
}

func (o optionBaseFee) Encode(encoder scale.Encoder) error {
	return encoder.EncodeOption(o.HasValue, o.Value)
}

func (o *optionBaseFee) Decode(decoder scale.Decoder) error {
	return decoder.DecodeOption(&o.HasValue, &o.Value)
}

type Header struct {
	Fields headerSCALE
	header *etypes.Header
}

func (h *Header) Decode(decoder scale.Decoder) error {
	var fields headerSCALE
	err := decoder.Decode(&fields)
	if err != nil {
		return err
	}

	h.Fields = fields
	return nil
}

func (h Header) Encode(encoder scale.Encoder) error {
	return encoder.Encode(h.Fields)
}

func (h *Header) ID() HeaderID {
	return HeaderID{
		Number: h.Fields.Number,
		Hash:   types.NewH256(h.header.Hash().Bytes()),
	}
}

func MakeHeaderData(gethheader *etypes.Header) (*Header, error) {
	// Convert Geth types to their Substrate Go client counterparts that match our node
	var blockNumber uint64
	if !gethheader.Number.IsUint64() {
		return nil, fmt.Errorf("gethheader.Number is not uint64")
	}
	blockNumber = gethheader.Number.Uint64()

	var gasUsed, gasLimit big.Int
	gasUsed.SetUint64(gethheader.GasUsed)
	gasLimit.SetUint64(gethheader.GasLimit)

	var bloomBytes [256]byte
	copy(bloomBytes[:], gethheader.Bloom.Bytes())

	mixHashRLP, err := rlp.EncodeToBytes(gethheader.MixDigest)
	if err != nil {
		return nil, err
	}

	nonceRLP, err := rlp.EncodeToBytes(gethheader.Nonce)
	if err != nil {
		return nil, err
	}

	var baseFee optionBaseFee
	if gethheader.BaseFee == nil {
		baseFee = optionBaseFee{false, types.U256{}}
	} else {
		baseFee = optionBaseFee{true, types.NewU256(*gethheader.BaseFee)}
	}

	return &Header{
		Fields: headerSCALE{
			ParentHash:       types.NewH256(gethheader.ParentHash.Bytes()),
			Timestamp:        types.NewU64(gethheader.Time),
			Number:           types.NewU64(blockNumber),
			Author:           types.NewH160(gethheader.Coinbase.Bytes()),
			TransactionsRoot: types.NewH256(gethheader.TxHash.Bytes()),
			OmmersHash:       types.NewH256(gethheader.UncleHash.Bytes()),
			ExtraData:        types.NewBytes(gethheader.Extra),
			StateRoot:        types.NewH256(gethheader.Root.Bytes()),
			ReceiptsRoot:     types.NewH256(gethheader.ReceiptHash.Bytes()),
			LogsBloom:        types.NewBytes256(bloomBytes),
			GasUsed:          types.NewU256(gasUsed),
			GasLimit:         types.NewU256(gasLimit),
			Difficulty:       types.NewU256(*gethheader.Difficulty),
			Seal:             []types.Bytes{mixHashRLP, nonceRLP},
			BaseFee:          baseFee,
		},
		header: gethheader,
	}, nil
}
