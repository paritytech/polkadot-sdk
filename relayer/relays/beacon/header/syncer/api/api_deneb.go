package api

import (
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/scale"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/state"
	"github.com/snowfork/snowbridge/relayer/relays/util"
	"math/big"
)

func DenebExecutionPayloadToScale(e *state.ExecutionPayloadDeneb) (scale.ExecutionPayloadHeaderDeneb, error) {
	var payloadHeader scale.ExecutionPayloadHeaderDeneb
	transactionsContainer := state.TransactionsRootContainer{}
	transactionsContainer.Transactions = e.Transactions

	transactionsRoot, err := transactionsContainer.HashTreeRoot()
	if err != nil {
		return payloadHeader, err
	}

	var withdrawalRoot types.H256

	withdrawalContainer := state.WithdrawalsRootContainerMainnet{}
	withdrawalContainer.Withdrawals = e.Withdrawals
	withdrawalRoot, err = withdrawalContainer.HashTreeRoot()
	if err != nil {
		return payloadHeader, err
	}

	baseFeePerGas := big.Int{}
	// Change BaseFeePerGas back from little-endian to big-endian
	baseFeePerGas.SetBytes(util.ChangeByteOrder(e.BaseFeePerGas[:]))

	return scale.ExecutionPayloadHeaderDeneb{
		ParentHash:       types.NewH256(e.ParentHash[:]),
		FeeRecipient:     e.FeeRecipient,
		StateRoot:        types.NewH256(e.StateRoot[:]),
		ReceiptsRoot:     types.NewH256(e.ReceiptsRoot[:]),
		LogsBloom:        e.LogsBloom[:],
		PrevRandao:       types.NewH256(e.PrevRandao[:]),
		BlockNumber:      types.NewU64(e.BlockNumber),
		GasLimit:         types.NewU64(e.GasLimit),
		GasUsed:          types.NewU64(e.GasUsed),
		Timestamp:        types.NewU64(e.Timestamp),
		ExtraData:        e.ExtraData,
		BaseFeePerGas:    types.NewU256(baseFeePerGas),
		BlockHash:        types.NewH256(e.BlockHash[:]),
		TransactionsRoot: transactionsRoot,
		WithdrawalsRoot:  withdrawalRoot,
		BlobGasUsed:      types.NewU64(e.BlobGasUsed),
		ExcessBlobGas:    types.NewU64(e.ExcessBlobGas),
	}, nil
}
