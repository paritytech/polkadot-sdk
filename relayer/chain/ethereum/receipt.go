// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package ethereum

import (
	"context"
	"sync"

	etypes "github.com/ethereum/go-ethereum/core/types"
	"golang.org/x/sync/errgroup"
)

const receiptFetchBatchSize int = 100

// Fetch all receipts for the given block in batches of `receiptFetchBatchSize`
func GetAllReceipts(ctx context.Context, conn *Connection, block *etypes.Block) (etypes.Receipts, error) {
	transactions := block.Body().Transactions
	numTransactions := len(transactions)
	receiptsByIndex := sync.Map{}

	for i := 0; i < numTransactions; i += receiptFetchBatchSize {
		eg, ctx := errgroup.WithContext(ctx)
		upper := i + receiptFetchBatchSize
		if upper >= numTransactions {
			upper = numTransactions
		}
		for j, tx := range transactions[i:upper] {
			index := i + j
			txHash := tx.Hash()
			eg.Go(func() error {
				receipt, err := conn.client.TransactionReceipt(ctx, txHash)
				if err != nil {
					return err
				}
				receiptsByIndex.Store(index, receipt)
				return nil
			})
		}
		err := eg.Wait()
		if err != nil {
			return nil, err
		}
	}

	// Place receipts in same order as corresponding transactions
	receipts := make([]*etypes.Receipt, numTransactions)
	receiptsByIndex.Range(func(index interface{}, receipt interface{}) bool {
		receipts[index.(int)] = receipt.(*etypes.Receipt)
		return true
	})
	return receipts, nil
}
