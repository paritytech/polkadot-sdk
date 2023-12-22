package beefy

import (
	"encoding/hex"
	"fmt"
	"math/big"

	"github.com/sirupsen/logrus"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/contracts"
	"github.com/snowfork/snowbridge/relayer/crypto/keccak"
	"github.com/snowfork/snowbridge/relayer/crypto/merkle"
)

type InitialRequestParams struct {
	Commitment contracts.BeefyClientCommitment
	Bitfield   []*big.Int
	Proof      contracts.BeefyClientValidatorProof
}

type FinalRequestParams struct {
	Commitment     contracts.BeefyClientCommitment
	Bitfield       []*big.Int
	Proofs         []contracts.BeefyClientValidatorProof
	Leaf           contracts.BeefyClientMMRLeaf
	LeafProof      [][32]byte
	LeafProofOrder *big.Int
}

func (r *Request) CommitmentHash() (*[32]byte, error) {
	commitmentBytes, err := types.EncodeToBytes(r.SignedCommitment.Commitment)
	if err != nil {
		return nil, err
	}

	commitmentHash := (&keccak.Keccak256{}).Hash(commitmentBytes)

	var commitmentHash32 [32]byte
	copy(commitmentHash32[:], commitmentHash[0:32])

	return &commitmentHash32, nil
}

// Generate RequestParams which contains merkle proof by validator's index
// together with the signature which will be verified in BeefyClient contract later
func (r *Request) MakeSubmitInitialParams(valAddrIndex int64, initialBitfield []*big.Int) (*InitialRequestParams, error) {
	commitment := toBeefyClientCommitment(&r.SignedCommitment.Commitment)

	proof, err := r.generateValidatorAddressProof(valAddrIndex)
	if err != nil {
		return nil, fmt.Errorf("generate validator proof: %w", err)
	}

	ok, validatorSignature := r.SignedCommitment.Signatures[valAddrIndex].Unwrap()
	if !ok {
		return nil, fmt.Errorf("signature is empty")
	}

	validatorAddress, err := r.Validators[valAddrIndex].IntoEthereumAddress()
	if err != nil {
		return nil, fmt.Errorf("convert to ethereum address: %w", err)
	}

	v, _r, s := cleanSignature(validatorSignature)

	msg := InitialRequestParams{
		Commitment: *commitment,
		Bitfield:   initialBitfield,
		Proof: contracts.BeefyClientValidatorProof{
			V:       v,
			R:       _r,
			S:       s,
			Index:   big.NewInt(valAddrIndex),
			Account: validatorAddress,
			Proof:   proof,
		},
	}

	return &msg, nil
}

func toBeefyClientCommitment(c *types.Commitment) *contracts.BeefyClientCommitment {
	return &contracts.BeefyClientCommitment{
		BlockNumber:    c.BlockNumber,
		ValidatorSetID: c.ValidatorSetID,
		Payload:        toBeefyPayload(c.Payload),
	}
}

func cleanSignature(input types.BeefySignature) (uint8, [32]byte, [32]byte) {
	// Update signature format (Polkadot uses recovery IDs 0 or 1, Eth uses 27 or 28, so we need to add 27)
	// Split signature into r, s, v and add 27 to v
	r := *(*[32]byte)(input[:32])
	s := *(*[32]byte)(input[32:64])
	v := byte(uint8(input[64]) + 27)
	return v, r, s
}

func (r *Request) generateValidatorAddressProof(validatorIndex int64) ([][32]byte, error) {
	leaves := make([][]byte, len(r.Validators))
	for i, rawAddress := range r.Validators {
		address, err := rawAddress.IntoEthereumAddress()
		if err != nil {
			return nil, fmt.Errorf("convert to ethereum address: %w", err)
		}
		leaves[i] = address.Bytes()
	}

	_, _, proof, err := merkle.GenerateMerkleProof(leaves, validatorIndex)
	if err != nil {
		return nil, err
	}

	return proof, nil
}

func (r *Request) MakeSubmitFinalParams(validatorIndices []uint64, initialBitfield []*big.Int) (*FinalRequestParams, error) {
	validatorProofs := []contracts.BeefyClientValidatorProof{}

	for _, validatorIndex := range validatorIndices {
		ok, beefySig := r.SignedCommitment.Signatures[validatorIndex].Unwrap()
		if !ok {
			return nil, fmt.Errorf("signature is empty")
		}

		v, _r, s := cleanSignature(beefySig)
		account, err := r.Validators[validatorIndex].IntoEthereumAddress()
		if err != nil {
			return nil, fmt.Errorf("convert to ethereum address: %w", err)
		}

		merkleProof, err := r.generateValidatorAddressProof(int64(validatorIndex))
		if err != nil {
			return nil, err
		}

		validatorProofs = append(validatorProofs, contracts.BeefyClientValidatorProof{
			V:       v,
			R:       _r,
			S:       s,
			Index:   new(big.Int).SetUint64(validatorIndex),
			Account: account,
			Proof:   merkleProof,
		})
	}

	commitment := contracts.BeefyClientCommitment{
		Payload:        toBeefyPayload(r.SignedCommitment.Commitment.Payload),
		BlockNumber:    r.SignedCommitment.Commitment.BlockNumber,
		ValidatorSetID: r.SignedCommitment.Commitment.ValidatorSetID,
	}

	inputLeaf := contracts.BeefyClientMMRLeaf{}

	var merkleProofItems [][32]byte

	proofOrder := new(big.Int)

	if r.IsHandover {
		inputLeaf = contracts.BeefyClientMMRLeaf{
			Version:              uint8(r.Proof.Leaf.Version),
			ParentNumber:         uint32(r.Proof.Leaf.ParentNumberAndHash.ParentNumber),
			ParentHash:           r.Proof.Leaf.ParentNumberAndHash.Hash,
			ParachainHeadsRoot:   r.Proof.Leaf.ParachainHeads,
			NextAuthoritySetID:   uint64(r.Proof.Leaf.BeefyNextAuthoritySet.ID),
			NextAuthoritySetLen:  uint32(r.Proof.Leaf.BeefyNextAuthoritySet.Len),
			NextAuthoritySetRoot: r.Proof.Leaf.BeefyNextAuthoritySet.Root,
		}
		for _, mmrProofItem := range r.Proof.MerkleProofItems {
			merkleProofItems = append(merkleProofItems, mmrProofItem)
		}
		proofOrder = proofOrder.SetUint64(r.Proof.MerkleProofOrder)
	}

	msg := FinalRequestParams{
		Commitment:     commitment,
		Bitfield:       initialBitfield,
		Proofs:         validatorProofs,
		Leaf:           inputLeaf,
		LeafProof:      merkleProofItems,
		LeafProofOrder: proofOrder,
	}

	return &msg, nil
}

func toBeefyPayload(items []types.PayloadItem) []contracts.BeefyClientPayloadItem {
	beefyItems := make([]contracts.BeefyClientPayloadItem, len(items))
	for i, item := range items {
		beefyItems[i] = contracts.BeefyClientPayloadItem{
			PayloadID: item.ID,
			Data:      item.Data,
		}
	}

	return beefyItems
}

func commitmentToLog(commitment contracts.BeefyClientCommitment) logrus.Fields {
	payloadFields := make([]logrus.Fields, len(commitment.Payload))
	for i, payloadItem := range commitment.Payload {
		payloadFields[i] = logrus.Fields{
			"payloadID": string(rune(payloadItem.PayloadID[0])) + string(rune(payloadItem.PayloadID[1])),
			"data":      "0x" + hex.EncodeToString(payloadItem.Data),
		}
	}

	return logrus.Fields{
		"blockNumber":    commitment.BlockNumber,
		"validatorSetID": commitment.ValidatorSetID,
		"payload":        payloadFields,
	}
}

func bitfieldToStrings(bitfield []*big.Int) []string {
	strings := make([]string, len(bitfield))
	for i, subfield := range bitfield {
		strings[i] = fmt.Sprintf("%0256b", subfield)
	}

	return strings
}

func proofToLog(proof contracts.BeefyClientValidatorProof) logrus.Fields {
	hexProof := make([]string, len(proof.Proof))
	for i, proof := range proof.Proof {
		hexProof[i] = Hex(proof[:])
	}

	return logrus.Fields{
		"V":       proof.V,
		"R":       Hex(proof.R[:]),
		"S":       Hex(proof.S[:]),
		"Index":   proof.Index.Uint64(),
		"Account": proof.Account.Hex(),
		"Proof":   hexProof,
	}
}
