package parachain

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"sort"

	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/chain/relaychain"
	"github.com/snowfork/snowbridge/relayer/crypto/merkle"
)

// ByLeafIndex implements sort.Interface based on the LeafIndex field.
type ByParaID []relaychain.ParaHead

func (b ByParaID) Len() int           { return len(b) }
func (b ByParaID) Less(i, j int) bool { return b[i].ParaID < b[j].ParaID }
func (b ByParaID) Swap(i, j int)      { b[i], b[j] = b[j], b[i] }

type MerkleProofData struct {
	PreLeaves       PreLeaves `json:"preLeaves"`
	NumberOfLeaves  int       `json:"numberOfLeaves"`
	ProvenPreLeaf   HexBytes  `json:"provenPreLeaf"`
	ProvenLeaf      HexBytes  `json:"provenLeaf"`
	ProvenLeafIndex int64     `json:"provenLeafIndex"`
	Root            HexBytes  `json:"root"`
	Proof           Proof     `json:"proof"`
}

type PreLeaves [][]byte
type Proof [][32]byte
type HexBytes []byte

func (h HexBytes) MarshalJSON() ([]byte, error) {
	b, _ := json.Marshal("0x" + hex.EncodeToString(h))
	return b, nil
}

func (h HexBytes) String() string {
	b, _ := json.Marshal(h)
	return string(b)
}

func (h HexBytes) Hex() string {
	return "0x" + hex.EncodeToString(h)
}

func (d PreLeaves) MarshalJSON() ([]byte, error) {
	items := make([]string, 0, len(d))
	for _, v := range d {
		items = append(items, "0x"+hex.EncodeToString(v))
	}
	b, _ := json.Marshal(items)
	return b, nil
}

func (d Proof) MarshalJSON() ([]byte, error) {
	items := make([]string, 0, len(d))
	for _, v := range d {
		items = append(items, "0x"+hex.EncodeToString(v[:]))
	}
	b, _ := json.Marshal(items)
	return b, nil
}

func (d MerkleProofData) String() string {
	b, _ := json.Marshal(d)
	return string(b)
}

func CreateParachainMerkleProof(heads []relaychain.ParaHead, paraID uint32) (MerkleProofData, error) {
	// sort slice by para ID
	sort.Sort(ByParaID(heads))

	// loop headers, convert to pre leaves and find header being proven
	preLeaves := make([][]byte, 0, len(heads))
	var headerToProve []byte
	var headerIndex int64
	for i, head := range heads {
		preLeaf, err := types.EncodeToBytes(head)
		if err != nil {
			return MerkleProofData{}, err
		}
		preLeaves = append(preLeaves, preLeaf)
		if head.ParaID == paraID {
			headerToProve = preLeaf
			headerIndex = int64(i)
		}
	}

	// Reference implementation of MerkleTree in substrate
	// https://github.com/paritytech/substrate/blob/ea387c634715793f806286abf1e64cabf9b7026f/frame/beefy-mmr/primitives/src/lib.rs#L45-L54
	leaf, root, proof, err := merkle.GenerateMerkleProof(preLeaves, headerIndex)
	if err != nil {
		return MerkleProofData{}, fmt.Errorf("create parachain merkle proof: %w", err)
	}

	return MerkleProofData{
		PreLeaves:       preLeaves,
		NumberOfLeaves:  len(preLeaves),
		ProvenPreLeaf:   headerToProve,
		ProvenLeaf:      leaf,
		ProvenLeafIndex: headerIndex,
		Root:            root,
		Proof:           proof,
	}, nil
}
