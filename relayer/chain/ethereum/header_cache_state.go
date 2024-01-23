// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package ethereum

import (
	"context"
	"fmt"
	"math"

	gethCommon "github.com/ethereum/go-ethereum/common"
	gethTypes "github.com/ethereum/go-ethereum/core/types"
	gethTrie "github.com/ethereum/go-ethereum/trie"
	"golang.org/x/sync/errgroup"
)

type BlockLoader interface {
	GetBlock(ctx context.Context, hash gethCommon.Hash) (*gethTypes.Block, error)
	GetAllReceipts(ctx context.Context, block *gethTypes.Block) (gethTypes.Receipts, error)
}

type DefaultBlockLoader struct {
	Conn *Connection
}

func (d *DefaultBlockLoader) GetBlock(ctx context.Context, hash gethCommon.Hash) (*gethTypes.Block, error) {
	return d.Conn.client.BlockByHash(ctx, hash)
}

func (d *DefaultBlockLoader) GetAllReceipts(ctx context.Context, block *gethTypes.Block) (gethTypes.Receipts, error) {
	return GetAllReceipts(ctx, d.Conn, block)
}

// Keeps the blocks and receipts for the latest block heights / numbers
// in memory (up to `capacity` block numbers)
type BlockCache struct {
	capacity       int
	hashesByNumber map[uint64][]string
	blocks         map[string]*gethTypes.Block
	receiptTries   map[string]*gethTrie.Trie
}

func NewBlockCache(capacity int) *BlockCache {
	return &BlockCache{
		capacity:       capacity,
		hashesByNumber: make(map[uint64][]string, capacity),
		blocks:         make(map[string]*gethTypes.Block, capacity),
		receiptTries:   make(map[string]*gethTrie.Trie, capacity),
	}
}

func (bc *BlockCache) Insert(block *gethTypes.Block, receiptTrie *gethTrie.Trie) {
	hash := block.Hash().Hex()
	_, exists := bc.blocks[hash]
	if exists {
		return
	}

	number := block.Number().Uint64()
	hashesAtNumber, numberExists := bc.hashesByNumber[number]
	// Remove oldest blocks if we've reached capacity
	if !numberExists && len(bc.hashesByNumber) == bc.capacity {
		var minNumber uint64 = math.MaxUint64
		for number := range bc.hashesByNumber {
			if number < minNumber {
				minNumber = number
			}
		}

		hashesToRemove := bc.hashesByNumber[minNumber]
		delete(bc.hashesByNumber, minNumber)
		for _, hashToRemove := range hashesToRemove {
			delete(bc.blocks, hashToRemove)
			delete(bc.receiptTries, hashToRemove)
		}

	}

	bc.blocks[hash] = block
	bc.receiptTries[hash] = receiptTrie
	if numberExists {
		bc.hashesByNumber[number] = append(hashesAtNumber, hash)
	} else {
		bc.hashesByNumber[number] = []string{hash}
	}
}

func (bc *BlockCache) Get(hash gethCommon.Hash) (*gethTypes.Block, *gethTrie.Trie, bool) {
	hashHex := hash.Hex()
	block, exists := bc.blocks[hashHex]
	if exists {
		return block, bc.receiptTries[hashHex], true
	}
	return nil, nil, false
}

// HeaderCache fetches and caches data we need to construct proofs
// as we move along the Ethereum chain.
type HeaderCache struct {
	blockLoader BlockLoader
	blockCache  *BlockCache
	eg          *errgroup.Group
}

// Instantiates a Header Cache with just a block loader and block cache.
// Used by beacon relayer.
func NewHeaderBlockCache(
	bl BlockLoader,
) (*HeaderCache, error) {
	blockCache := NewBlockCache(5)
	blockLoader := bl
	if blockLoader == nil {
		return nil, fmt.Errorf("BlockLoader param is nil")
	}

	state := HeaderCache{
		blockCache:  blockCache,
		blockLoader: blockLoader,
	}
	return &state, nil
}

// GetReceiptTrie returns a Merkle Patricia trie constructed from the receipts
// of the block specified by `hash`. If the trie isn't cached, it will block for
// multiple seconds to fetch receipts and construct the trie.
func (s *HeaderCache) GetReceiptTrie(ctx context.Context, hash gethCommon.Hash) (*gethTrie.Trie, error) {
	_, receiptTrie, exists := s.blockCache.Get(hash)
	if exists {
		return receiptTrie, nil
	}

	block, err := s.blockLoader.GetBlock(ctx, hash)
	if err != nil {
		return nil, fmt.Errorf("get block: %w", err)
	}

	receipts, err := s.blockLoader.GetAllReceipts(ctx, block)
	if err != nil {
		return nil, fmt.Errorf("get all receipts: %w", err)
	}

	receiptTrie, err = MakeTrie(receipts)
	if err != nil {
		return nil, fmt.Errorf("make trie: %w", err)
	}

	if receiptTrie.Hash() != block.ReceiptHash() {
		return nil, fmt.Errorf("receipt trie does not match block receipt hash")
	}

	s.blockCache.Insert(block, receiptTrie)
	return receiptTrie, nil
}
