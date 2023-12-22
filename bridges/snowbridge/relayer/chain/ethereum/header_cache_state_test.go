package ethereum_test

import (
	"context"
	"encoding/json"
	"fmt"
	"math/big"
	"testing"

	gethCommon "github.com/ethereum/go-ethereum/common"
	gethTypes "github.com/ethereum/go-ethereum/core/types"
	gethTrie "github.com/ethereum/go-ethereum/trie"
	"github.com/snowfork/ethashproof"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"
	"golang.org/x/sync/errgroup"
)

type TestBlockData struct {
	Hash     gethCommon.Hash
	Block    *gethTypes.Block
	Receipts gethTypes.Receipts
}

func makeTestBlockData(num byte) []*TestBlockData {
	data := make([]*TestBlockData, 0, num)
	emptyRoot := gethCommon.HexToHash("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421")

	for i := byte(0); i < num; i++ {
		header := gethTypes.Header{
			Number:      big.NewInt(int64(i)),
			TxHash:      emptyRoot,
			ReceiptHash: emptyRoot,
		}
		if len(data) > 0 {
			header.ParentHash = data[i-1].Hash
		}
		datum := TestBlockData{
			Hash:     header.Hash(),
			Block:    gethTypes.NewBlockWithHeader(&header),
			Receipts: []*gethTypes.Receipt{},
		}
		data = append(data, &datum)
	}

	return data
}

func block11408438() *gethTypes.Block {
	rawData := readTestData("header11408438.json")
	var header gethTypes.Header
	err := json.Unmarshal(rawData, &header)
	if err != nil {
		panic(err)
	}

	rawData = readTestData("transactions11408438.json")
	transactions := make([]*gethTypes.Transaction, 130)
	err = json.Unmarshal(rawData, &transactions)
	if err != nil {
		panic(err)
	}

	block := gethTypes.NewBlockWithHeader(&header).WithBody(transactions, []*gethTypes.Header{})
	// Sanity check for match between header and transaction data
	expectedTxHash := gethCommon.HexToHash("84d7fb20eff54c19387510fd646e16c3bc0879545799595b78bb2129242d770c")
	if gethTypes.DeriveSha(block.Transactions(), new(gethTrie.Trie)) != expectedTxHash ||
		block.TxHash() != expectedTxHash {
		panic(fmt.Errorf("Geth header transaction hash doesn't match block transactions"))
	}

	return block
}

func receipts11408438() gethTypes.Receipts {
	rawData := readTestData("receipts11408438.json")
	receipts := make([]*gethTypes.Receipt, 130)
	err := json.Unmarshal(rawData, &receipts)
	if err != nil {
		panic(err)
	}

	return receipts
}

type TestEthashproofCacheLoader struct {
	mock.Mock
}

func (tcl *TestEthashproofCacheLoader) MakeCache(epoch uint64) (*ethashproof.DatasetMerkleTreeCache, error) {
	args := tcl.Called(epoch)
	return args.Get(0).(*ethashproof.DatasetMerkleTreeCache), args.Error(1)
}

type TestBlockLoader struct {
	mock.Mock
}

func (tbl *TestBlockLoader) GetBlock(ctx context.Context, hash gethCommon.Hash) (*gethTypes.Block, error) {
	args := tbl.Called(hash)
	return args.Get(0).(*gethTypes.Block), args.Error(1)
}

func (tbl *TestBlockLoader) GetAllReceipts(ctx context.Context, block *gethTypes.Block) (gethTypes.Receipts, error) {
	args := tbl.Called(block)
	return args.Get(0).(gethTypes.Receipts), args.Error(1)
}

func TestHeaderCacheState_EthashproofCacheLoading(t *testing.T) {
	eg := &errgroup.Group{}
	cacheLoader := TestEthashproofCacheLoader{}
	cacheLoader.On("MakeCache", uint64(0)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 0}, nil)
	cacheLoader.On("MakeCache", uint64(1)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 1}, nil)
	cacheLoader.On("MakeCache", uint64(2)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 2}, nil)
	cacheLoader.On("MakeCache", uint64(3)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 3}, nil)

	// Should load epoch 0 and 1 caches
	hcs, err := ethereum.NewHeaderCache("", "", eg, 0, &TestBlockLoader{}, &cacheLoader)
	if err != nil {
		panic(err)
	}
	err = eg.Wait()
	if err != nil {
		panic(err)
	}

	cacheLoader.AssertCalled(t, "MakeCache", uint64(0))
	cacheLoader.AssertCalled(t, "MakeCache", uint64(1))
	cacheLoader.AssertNumberOfCalls(t, "MakeCache", 2)

	// No new cache data needs to be loaded
	cache := getCacheAndWait(eg, hcs, 29999)
	assert.Equal(t, cache.Epoch, uint64(0))
	cacheLoader.AssertNumberOfCalls(t, "MakeCache", 2)

	// Should trigger epoch 2 to be loaded
	cache = getCacheAndWait(eg, hcs, 30000)
	assert.Equal(t, cache.Epoch, uint64(1))
	cacheLoader.AssertCalled(t, "MakeCache", uint64(2))
	cacheLoader.AssertNumberOfCalls(t, "MakeCache", 3)

	// Should trigger epoch 0 to be loaded again
	cache = getCacheAndWait(eg, hcs, 29999)
	assert.Equal(t, cache.Epoch, uint64(0))
	cacheLoader.AssertNumberOfCalls(t, "MakeCache", 4)

	// Should trigger epoch 2 and 3 to be loaded
	cache = getCacheAndWait(eg, hcs, 60000)
	assert.Equal(t, cache.Epoch, uint64(2))
	cacheLoader.AssertCalled(t, "MakeCache", uint64(3))
	cacheLoader.AssertNumberOfCalls(t, "MakeCache", 6)
}

func TestHeaderCacheState_BlockCacheLoading(t *testing.T) {
	ctx := context.Background()
	cacheLoader := TestEthashproofCacheLoader{}
	cacheLoader.On("MakeCache", uint64(0)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 0}, nil)
	cacheLoader.On("MakeCache", uint64(1)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 1}, nil)
	blockLoader := TestBlockLoader{}
	data := makeTestBlockData(6)
	for _, datum := range data {
		blockLoader.On("GetBlock", datum.Hash).Return(datum.Block, nil)
		blockLoader.On("GetAllReceipts", datum.Block).Return(datum.Receipts, nil)
	}

	hcs, err := ethereum.NewHeaderCache("", "", &errgroup.Group{}, 0, &blockLoader, &cacheLoader)
	if err != nil {
		panic(err)
	}

	trie, err := hcs.GetReceiptTrie(ctx, data[0].Hash)
	assert.NotNil(t, trie)
	assert.Nil(t, err)
	blockLoader.AssertCalled(t, "GetBlock", data[0].Hash)
	blockLoader.AssertNumberOfCalls(t, "GetBlock", 1)
	blockLoader.AssertCalled(t, "GetAllReceipts", data[0].Block)
	blockLoader.AssertNumberOfCalls(t, "GetAllReceipts", 1)

	// Previous block/receipt should be cached
	trie, err = hcs.GetReceiptTrie(ctx, data[0].Hash)
	assert.NotNil(t, trie)
	assert.Nil(t, err)
	blockLoader.AssertNumberOfCalls(t, "GetBlock", 1)
	blockLoader.AssertNumberOfCalls(t, "GetAllReceipts", 1)

	// Should generate 5 more calls since none of these are cached
	_, _ = hcs.GetReceiptTrie(ctx, data[1].Hash)
	_, _ = hcs.GetReceiptTrie(ctx, data[2].Hash)
	_, _ = hcs.GetReceiptTrie(ctx, data[3].Hash)
	_, _ = hcs.GetReceiptTrie(ctx, data[4].Hash)
	_, _ = hcs.GetReceiptTrie(ctx, data[5].Hash)
	blockLoader.AssertNumberOfCalls(t, "GetBlock", 6)
	blockLoader.AssertNumberOfCalls(t, "GetAllReceipts", 6)

	// Should have been deleted in cache because capacity = 5 was reached above
	_, _ = hcs.GetReceiptTrie(ctx, data[0].Hash)
	blockLoader.AssertNumberOfCalls(t, "GetBlock", 7)
	blockLoader.AssertNumberOfCalls(t, "GetAllReceipts", 7)
}

func TestHeaderCacheState_ReceiptTrieCreation(t *testing.T) {
	block := block11408438()
	receipts := receipts11408438()
	cacheLoader := TestEthashproofCacheLoader{}
	cacheLoader.On("MakeCache", uint64(0)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 0}, nil)
	cacheLoader.On("MakeCache", uint64(1)).Return(&ethashproof.DatasetMerkleTreeCache{Epoch: 1}, nil)
	blockLoader := TestBlockLoader{}
	blockLoader.On("GetBlock", block.Hash()).Return(block, nil)
	blockLoader.On("GetAllReceipts", block).Return(receipts, nil)

	hcs, err := ethereum.NewHeaderCache("", "", &errgroup.Group{}, 0, &blockLoader, &cacheLoader)
	if err != nil {
		panic(err)
	}

	// Check that our receipt trie matches Geth
	receiptTrie, err := hcs.GetReceiptTrie(context.Background(), block.Hash())
	assert.Nil(t, err)
	assert.Equal(t, receiptTrie.Hash(), block.ReceiptHash())
}

func getCacheAndWait(
	eg *errgroup.Group,
	hcs *ethereum.HeaderCache,
	blockNumber uint64,
) *ethashproof.DatasetMerkleTreeCache {
	cache, err := hcs.MakeEthashproofCache(blockNumber)
	if err != nil {
		panic(err)
	}

	err = eg.Wait()
	if err != nil {
		panic(err)
	}

	return cache
}
