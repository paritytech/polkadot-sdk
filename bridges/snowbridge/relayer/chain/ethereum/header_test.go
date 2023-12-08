package ethereum_test

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"

	ecommon "github.com/ethereum/go-ethereum/common"
	etypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/snowfork/ethashproof"
	"github.com/snowfork/go-substrate-rpc-client/v4/scale"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/stretchr/testify/assert"
)

func encodeToBytes(value interface{}) ([]byte, error) {
	var buffer = bytes.Buffer{}
	err := scale.NewEncoder(&buffer).Encode(value)
	if err != nil {
		return buffer.Bytes(), err
	}
	return buffer.Bytes(), nil
}

func decodeFromBytes(bz []byte, target interface{}) error {
	return scale.NewDecoder(bytes.NewReader(bz)).Decode(target)
}

// To retrieve test data:
// curl https://mainnet.infura.io/v3/<PROJECT_ID> \
//     -X POST \
//     -H "Content-Type: application/json" \
//     -d '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params": ["0xA93972",false],"id":1}'

func gethHeader11090290() etypes.Header {
	json := `{
		"difficulty": "0xbc140caa61087",
		"extraData": "0x65746865726d696e652d61736961312d33",
		"gasLimit": "0xbe8c19",
		"gasUsed": "0x0",
		"hash": "0x0f9bdc91c2e0140acb873330742bda8c8181fa3add91fe7ae046251679cedef7",
		"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
		"miner": "0xea674fdde714fd979de3edf0f56aa9716b898ec8",
		"mixHash": "0xbe3adfb0087be62b28b716e2cdf3c79329df5caa04c9eee035d35b5d52102815",
		"nonce": "0x6935bbe7b63c4f8e",
		"number": "0xa93972",
		"parentHash": "0xbede0bddd6f32c895fc505ffe0c39d9bde58e9a5272f31a3dee448b796edcbe3",
		"receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
		"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
		"size": "0x217",
		"stateRoot": "0x7dcb8aca872b712bad81df34a89d4efedc293566ffc3eeeb5cbcafcc703e42c9",
		"timestamp": "0x5f8e4b91",
		"totalDifficulty": "0x3d7b646a5ba5b2622ec",
		"transactionsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
	}`

	var header etypes.Header
	header.UnmarshalJSON([]byte(json))
	if header.Hash() != ecommon.HexToHash("0f9bdc91c2e0140acb873330742bda8c8181fa3add91fe7ae046251679cedef7") {
		panic(fmt.Errorf("Geth header hash doesn't match the expected hash"))
	}

	return header
}

func encodedProof11090290() []byte {
	rawData := readTestData("encodedProof11090290.json")
	var encoded []byte
	err := json.Unmarshal(rawData, &encoded)
	if err != nil {
		panic(err)
	}
	return encoded
}

func proofCache11090290() *ethashproof.DatasetMerkleTreeCache {
	rawData := readTestData("epochCache369.json")
	var cache ethashproof.DatasetMerkleTreeCache
	err := json.Unmarshal(rawData, &cache)
	if err != nil {
		panic(err)
	}
	return &cache
}

func TestHeader_EncodeDecode11090290(t *testing.T) {
	gethHeader := gethHeader11090290()
	// From header.encode() call in Substrate
	expectedEncoded := []byte{
		190, 222, 11, 221, 214, 243, 44, 137, 95, 197, 5, 255, 224, 195, 157, 155, 222, 88, 233, 165,
		39, 47, 49, 163, 222, 228, 72, 183, 150, 237, 203, 227, 145, 75, 142, 95, 0, 0, 0, 0, 114, 57,
		169, 0, 0, 0, 0, 0, 234, 103, 79, 221, 231, 20, 253, 151, 157, 227, 237, 240, 245, 106, 169,
		113, 107, 137, 142, 200, 86, 232, 31, 23, 27, 204, 85, 166, 255, 131, 69, 230, 146, 192, 248,
		110, 91, 72, 224, 27, 153, 108, 173, 192, 1, 98, 47, 181, 227, 99, 180, 33, 29, 204, 77, 232,
		222, 199, 93, 122, 171, 133, 181, 103, 182, 204, 212, 26, 211, 18, 69, 27, 148, 138, 116, 19,
		240, 161, 66, 253, 64, 212, 147, 71, 68, 101, 116, 104, 101, 114, 109, 105, 110, 101, 45, 97,
		115, 105, 97, 49, 45, 51, 125, 203, 138, 202, 135, 43, 113, 43, 173, 129, 223, 52, 168, 157, 78,
		254, 220, 41, 53, 102, 255, 195, 238, 235, 92, 188, 175, 204, 112, 62, 66, 201, 86, 232, 31, 23,
		27, 204, 85, 166, 255, 131, 69, 230, 146, 192, 248, 110, 91, 72, 224, 27, 153, 108, 173, 192, 1,
		98, 47, 181, 227, 99, 180, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 25, 140, 190, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 135, 16, 166, 202, 64, 193, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 132, 160, 190, 58, 223, 176, 8, 123, 230, 43,
		40, 183, 22, 226, 205, 243, 199, 147, 41, 223, 92, 170, 4, 201, 238, 224, 53, 211, 91, 93, 82,
		16, 40, 21, 36, 136, 105, 53, 187, 231, 182, 60, 79, 142, 0,
	}

	header, err := ethereum.MakeHeaderData(&gethHeader)
	if err != nil {
		panic(err)
	}

	encoded, err := encodeToBytes(header)
	if err != nil {
		panic(err)
	}
	assert.Equal(t, expectedEncoded, encoded, "Encoded ethereum.Header should match Substrate header")

	var decoded ethereum.Header
	err = decodeFromBytes(encoded, &decoded)
	if err != nil {
		panic(err)
	}
	assert.Equal(t, header.Fields, decoded.Fields, "Decoded Substrate header should match ethereum.Header")
}

func TestProof_EncodeDecode(t *testing.T) {
	t.Skip("Skipping test as it depends on external data.")

	gethHeader := gethHeader11090290()
	cache := proofCache11090290()
	expectedEncoded := encodedProof11090290()

	dataDir := "/tmp"

	proof, err := ethereum.MakeProofData(&gethHeader, cache, dataDir)
	if err != nil {
		panic(err)
	}

	encoded, err := encodeToBytes(proof)
	if err != nil {
		panic(err)
	}
	assert.Equal(t, expectedEncoded, encoded, "Encoded ethereum.DoubleNodeWithMerkleProof should match Substrate proof")

	var decoded []ethereum.DoubleNodeWithMerkleProof
	err = decodeFromBytes(encoded, &decoded)
	if err != nil {
		panic(err)
	}
	assert.Equal(t, proof, decoded, "Decoded Substrate proof should match ethereum.DoubleNodeWithMerkleProof")
}

func readTestData(filename string) []byte {
	dir, err := os.Getwd()
	if err != nil {
		panic(err)
	}

	rawData, err := ioutil.ReadFile(filepath.Join(dir, "testdata", filename))
	if err != nil {
		panic(err)
	}
	return rawData
}
