package util

import (
	"encoding/hex"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
)

func BytesBranchToScale(proofs [][]byte) []types.H256 {
	branch := []types.H256{}

	for _, proof := range proofs {
		branch = append(branch, types.NewH256(proof))
	}

	return branch
}

func ScaleBranchToString(proofs []types.H256) []string {
	branch := []string{}

	for _, proof := range proofs {
		branch = append(branch, proof.Hex())
	}

	return branch
}

func ProofBranchToScale(proofs []string) []types.H256 {
	branch := []types.H256{}

	for _, proof := range proofs {
		branch = append(branch, types.NewH256(common.FromHex(proof)))
	}

	return branch
}

func ToUint64(stringVal string) (uint64, error) {
	intVal, err := strconv.ParseUint(stringVal, 10, 64)
	if err != nil {
		return 0, err
	}

	return intVal, err
}

func ToUint64Array(items []types.U64) []uint64 {
	result := []uint64{}

	for _, item := range items {
		result = append(result, uint64(item))
	}

	return result
}
func HexStringToByteArray(hexString string) ([]byte, error) {
	bytes, err := hex.DecodeString(strings.Replace(hexString, "0x", "", 1))
	if err != nil {
		return []byte{}, err
	}

	return bytes, nil
}

func BytesToHexString(bytes []byte) string {
	return "0x" + hex.EncodeToString(bytes)
}

func HexStringToPublicKey(hexString string) ([48]byte, error) {
	var pubkeyBytes [48]byte
	key, err := hex.DecodeString(strings.Replace(hexString, "0x", "", 1))
	if err != nil {
		return [48]byte{}, err
	}

	copy(pubkeyBytes[:], key)

	return pubkeyBytes, nil
}

func HexStringTo32Bytes(hexString string) ([32]byte, error) {
	var pubkeyBytes [32]byte
	key, err := hex.DecodeString(strings.Replace(hexString, "0x", "", 1))
	if err != nil {
		return [32]byte{}, err
	}

	copy(pubkeyBytes[:], key)

	return pubkeyBytes, nil
}

func HexStringTo96Bytes(hexString string) ([96]byte, error) {
	var pubkeyBytes [96]byte
	key, err := hex.DecodeString(strings.Replace(hexString, "0x", "", 1))
	if err != nil {
		return [96]byte{}, err
	}

	copy(pubkeyBytes[:], key)

	return pubkeyBytes, nil
}

func HexStringTo20Bytes(hexString string) ([20]byte, error) {
	var pubkeyBytes [20]byte
	key, err := hex.DecodeString(strings.Replace(hexString, "0x", "", 1))
	if err != nil {
		return [20]byte{}, err
	}

	copy(pubkeyBytes[:], key)

	return pubkeyBytes, nil
}

func HexStringTo256Bytes(hexString string) ([256]byte, error) {
	var pubkeyBytes [256]byte
	key, err := hex.DecodeString(strings.Replace(hexString, "0x", "", 1))
	if err != nil {
		return [256]byte{}, err
	}

	copy(pubkeyBytes[:], key)

	return pubkeyBytes, nil
}

// ChangeByteOrder is used to convert a byte array to little endian or big endianness.
func ChangeByteOrder(b []byte) []byte {
	for i := 0; i < len(b)/2; i++ {
		b[i], b[len(b)-i-1] = b[len(b)-i-1], b[i]
	}

	return b
}
