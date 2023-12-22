// Code is based on a heavily modified version of https://github.com/wilfreddenton/merkle

package merkle

import (
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math"

	"github.com/snowfork/snowbridge/relayer/crypto/keccak"
)

// Position constants are used in merkle path nodes to denote
// whether the node was a left child or right child. This allows
// hash concatenation can be performed correctly.
const (
	PositionLeft  = "left"
	PositionRight = "right"
)

func depth(n int) int {
	return int(math.Ceil(math.Log2(float64(n))))
}

type Hasher interface {
	// Hash calculates the hash of a given input
	Hash([]byte) []byte
}

// Node is used to represent the steps of a merkle path.
// This structure is not used within the Tree structure.
type Node struct {
	Hash     []byte `json:"hash"`
	Position string `json:"position"`
}

// The Hash value is encoded into a base64 string
func (n *Node) MarshalJSON() ([]byte, error) {
	type Alias Node
	return json.Marshal(&struct {
		Hash string `json:"hash"`
		*Alias
	}{
		Hash:  base64.StdEncoding.EncodeToString(n.Hash),
		Alias: (*Alias)(n),
	})
}

// The Hash value is decoded from a base64 encoded string
func (n *Node) UnmarshalJSON(data []byte) error {
	type Alias Node
	aux := &struct {
		Hash string `json:"hash"`
		*Alias
	}{
		Alias: (*Alias)(n),
	}

	if err := json.Unmarshal(data, &aux); err != nil {
		return err
	}

	var err error
	n.Hash, err = base64.StdEncoding.DecodeString(aux.Hash)
	if err != nil {
		return err
	}

	return nil
}

// Tree is the merkle tree structure. It is implemented
// as an array of arrays of arrays of bytes:
//
//	[
//	  [ root digest ],
//	  [ digest, digest ],
//	  [ digest, digest, digest, digest],
//	  ...
//	  [ leaf, leaf, leaf, leaf, ... ]
//	]
type Tree struct {
	levels [][][]byte
	h      Hasher
}

func (t *Tree) findIndex(leaf []byte) int {
	if t.levels == nil {
		return -1
	}

	s := hex.EncodeToString(leaf)

	for i, l := range t.levels[t.Depth()] {
		if hex.EncodeToString(l) == s {
			return i
		}
	}

	return -1
}

// Root returns the root hash of the tree or nil if it hasn't been hashed.
func (t *Tree) Root() []byte {
	if t.levels == nil {
		return nil
	}

	return t.levels[0][0]
}

// Depth returns the number of edges from the root to the leaf nodes
func (t *Tree) Depth() int {
	if t.levels == nil {
		return 0
	}

	return len(t.levels) - 1
}

// MerklePath generates an authentication path for the leaf.
// If the leaf is not contained in the tree than the return
// value is nil.
func (t *Tree) MerklePath(preLeaf []byte) []*Node {
	leaf := t.h.Hash(preLeaf)
	index := t.findIndex(leaf)

	if index < 0 {
		return nil
	}

	d := t.Depth()
	var path []*Node

	for i := d; i > 0; i -= 1 {
		level := t.levels[i]
		levelLen := len(level)
		remainder := levelLen % 2
		nextIndex := index / 2

		// if index is the the last item in an odd length level promote
		if index == levelLen-1 && remainder != 0 {
			index = nextIndex
			continue
		}

		// if i is odd we want to get the left sibling
		if index%2 != 0 {
			path = append(path, &Node{Hash: level[index-1], Position: PositionLeft})
		} else {
			path = append(path, &Node{Hash: level[index+1], Position: PositionRight})
		}

		index = nextIndex
	}

	return path
}

// Hash creates a merkle tree from an array of pre-leaves.
// Pre-leaves are represented as an array of bytes.
func (t *Tree) Hash(preLeaves [][]byte, h Hasher) error {
	n := len(preLeaves)

	if n == 0 {
		return errors.New("cannot create tree with 0 pre leaves")
	}

	// set tree wide hash function
	t.h = h

	d := depth(n)
	t.levels = make([][][]byte, d+1)
	leaves := make([][]byte, n)

	for i, preLeaf := range preLeaves {
		leaves[i] = h.Hash(preLeaf)
	}

	t.levels[d] = leaves

	for i := d; i > 0; i -= 1 {
		level := t.levels[i]
		levelLen := len(level)
		remainder := levelLen % 2
		nextLevel := make([][]byte, levelLen/2+remainder)

		k := 0
		for j := 0; j < len(level)-1; j += 2 {
			left := level[j]
			right := level[j+1]

			nextLevel[k] = h.Hash(append(left, right...))
			k += 1
		}

		if remainder != 0 {
			nextLevel[k] = level[len(level)-1]
		}

		t.levels[i-1] = nextLevel
	}

	return nil
}

func NewTree() *Tree {
	return &Tree{levels: nil}
}

// Prove is used to confirm that a leaf is contained within a merkle tree.
// It does not require the full tree, only the leaf and root hashes, the
// merkle path, and the hash function used. The merkle path can be retrieved
// from a node in the P2P network that has a copy of the full tree.
func Prove(preLeaf, root []byte, path []*Node, h Hasher) bool {
	leaf := h.Hash(preLeaf)
	hash := leaf

	for _, node := range path {
		if node.Position == PositionLeft {
			hash = append(node.Hash, hash...)
		} else {
			hash = append(hash, node.Hash...)
		}

		hash = h.Hash(hash)
	}

	return hex.EncodeToString(hash) == hex.EncodeToString(root)
}

func GenerateMerkleProof(inputPreLeaves [][]byte, leafIndex int64) ([]byte, []byte, [][32]byte, error) {
	// Create the tree
	tree := NewTree()
	tree.Hash(inputPreLeaves, &keccak.Keccak256{})

	root := tree.Root()

	// Generate Merkle Proof for leaf at index leafIndex
	proof := tree.MerklePath(inputPreLeaves[leafIndex])

	// Verify the proof
	verified := Prove(inputPreLeaves[leafIndex], root, proof, &keccak.Keccak256{})
	if !verified {
		return nil, nil, nil, fmt.Errorf("failed to verify proof")
	}

	proofContents := make([][32]byte, len(proof))
	for i, node := range proof {
		var hash32Byte [32]byte
		copy(hash32Byte[:], node.Hash)
		proofContents[i] = hash32Byte
	}

	hasher := &keccak.Keccak256{}
	leaf := hasher.Hash(inputPreLeaves[leafIndex])

	return leaf, root, proofContents, nil
}
