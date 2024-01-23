package ethereum_test

import (
	"context"
	"encoding/json"
	"math/big"

	gethCommon "github.com/ethereum/go-ethereum/common"
	gethTypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/stretchr/testify/mock"
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
