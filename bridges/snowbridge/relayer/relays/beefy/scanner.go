package beefy

import (
	"context"
	"fmt"
	"time"

	log "github.com/sirupsen/logrus"
	gsrpc "github.com/snowfork/go-substrate-rpc-client/v4"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/crypto/keccak"
	"github.com/snowfork/snowbridge/relayer/crypto/merkle"
)

type ScanBlocksResult struct {
	BlockNumber uint64
	BlockHash   types.Hash
	Depth       uint64
	Error       error
}

func ScanBlocks(ctx context.Context, api *gsrpc.SubstrateAPI, startBlock uint64) (chan ScanBlocksResult, error) {
	results := make(chan ScanBlocksResult)
	go scanBlocks(ctx, api, startBlock, results)
	return results, nil
}

func scanBlocks(ctx context.Context, api *gsrpc.SubstrateAPI, startBlock uint64, out chan<- ScanBlocksResult) {
	defer close(out)

	sendError := func(err error) {
		select {
		case <-ctx.Done():
			return
		case out <- ScanBlocksResult{Error: err}:
		}
	}

	current := startBlock
	for {
		finalizedHash, err := api.RPC.Beefy.GetFinalizedHead()
		if err != nil {
			sendError(fmt.Errorf("fetch finalized head: %w", err))
			return
		}

		finalizedHeader, err := api.RPC.Chain.GetHeader(finalizedHash)
		if err != nil {
			sendError(fmt.Errorf("fetch header for finalised head %v: %w", finalizedHash.Hex(), err))
			return
		}

		finalizedBlockNumber := uint64(finalizedHeader.Number)
		if current > finalizedBlockNumber {
			select {
			case <-ctx.Done():
				return
			case <-time.After(3 * time.Second):
			}
			continue
		}

		blockHash, err := api.RPC.Chain.GetBlockHash(current)
		if err != nil {
			sendError(fmt.Errorf("fetch block hash: %w", err))
			return
		}

		select {
		case <-ctx.Done():
			return
		case out <- ScanBlocksResult{BlockNumber: current, BlockHash: blockHash, Depth: finalizedBlockNumber - current}:
		}

		current++
	}
}

type ScanCommitmentsResult struct {
	SignedCommitment types.SignedCommitment
	BlockNumber      uint64
	BlockHash        types.Hash
	Depth            uint64
	Error            error
}

func ScanCommitments(ctx context.Context, api *gsrpc.SubstrateAPI, startBlock uint64) (<-chan ScanCommitmentsResult, error) {
	out := make(chan ScanCommitmentsResult)
	go scanCommitments(ctx, api, startBlock, out)
	return out, nil
}

func scanCommitments(ctx context.Context, api *gsrpc.SubstrateAPI, startBlock uint64, out chan<- ScanCommitmentsResult) {
	defer close(out)

	sendError := func(err error) {
		select {
		case <-ctx.Done():
			return
		case out <- ScanCommitmentsResult{Error: err}:
		}
	}

	in, err := ScanBlocks(ctx, api, startBlock)
	if err != nil {
		sendError(err)
		return
	}

	for {
		select {
		case <-ctx.Done():
			out <- ScanCommitmentsResult{Error: ctx.Err()}
			close(out)
			return
		case result, ok := <-in:
			if !ok {
				return
			}

			if result.Error != nil {
				sendError(result.Error)
				return
			}

			block, err := api.RPC.Chain.GetBlock(result.BlockHash)
			if err != nil {
				sendError(fmt.Errorf("fetch block: %w", err))
				return
			}

			var commitment *types.SignedCommitment
			for j := range block.Justifications {
				sc := types.OptionalSignedCommitment{}
				// Filter justification by EngineID
				// https://github.com/paritytech/substrate/blob/55c64bcc2af5a6e5fc3eb245e638379ebe18a58d/primitives/beefy/src/lib.rs#L114
				if block.Justifications[j].EngineID() == "BEEF" {
					// Decode as SignedCommitment
					// https://github.com/paritytech/substrate/blob/bcee526a9b73d2df9d5dea0f1a17677618d70b8e/primitives/beefy/src/commitment.rs#L89
					err := types.DecodeFromBytes(block.Justifications[j].Payload(), &sc)
					if err != nil {
						sendError(fmt.Errorf("decode BEEFY signed commitment: %w", err))
						return
					}
					ok, value := sc.Unwrap()
					if ok {
						commitment = &value
					}
				}
			}

			if commitment == nil {
				continue
			}

			select {
			case <-ctx.Done():
				return
			case out <- ScanCommitmentsResult{BlockNumber: result.BlockNumber, BlockHash: result.BlockHash, SignedCommitment: *commitment, Depth: result.Depth}:
			}
		}
	}
}

type ScanSafeCommitmentsResult struct {
	SignedCommitment types.SignedCommitment
	MMRProof         merkle.SimplifiedMMRProof
	BlockHash        types.Hash
	Depth            uint64
	Error            error
}

func ScanSafeCommitments(ctx context.Context, meta *types.Metadata, api *gsrpc.SubstrateAPI, startBlock uint64) (<-chan ScanSafeCommitmentsResult, error) {
	out := make(chan ScanSafeCommitmentsResult)
	go scanSafeCommitments(ctx, meta, api, startBlock, out)
	return out, nil
}

func scanSafeCommitments(ctx context.Context, meta *types.Metadata, api *gsrpc.SubstrateAPI, startBlock uint64, out chan<- ScanSafeCommitmentsResult) {
	defer close(out)

	sendError := func(err error) {
		select {
		case <-ctx.Done():
			return
		case out <- ScanSafeCommitmentsResult{Error: err}:
		}
	}

	in, err := ScanCommitments(ctx, api, startBlock)
	if err != nil {
		sendError(err)
		return
	}

	for {
		select {
		case <-ctx.Done():
			return
		case result, ok := <-in:
			if !ok {
				return
			}

			if result.Error != nil {
				sendError(result.Error)
				return
			}

			blockNumber := result.SignedCommitment.Commitment.BlockNumber
			blockHash, err := api.RPC.Chain.GetBlockHash(uint64(blockNumber))
			if err != nil {
				sendError(fmt.Errorf("fetch block hash: %w", err))
				return
			}
			proofIsValid, proof, err := makeProof(meta, api, blockNumber, blockHash)
			if err != nil {
				sendError(fmt.Errorf("proof generation for block %v at hash %v: %w", blockNumber, blockHash.Hex(), err))
				return
			}

			if !proofIsValid {
				log.WithFields(log.Fields{
					"parentNumber":   blockNumber,
					"beefyBlockHash": blockHash,
					"validatorSetID": result.SignedCommitment.Commitment.ValidatorSetID,
				}).Info("Proof for leaf is invalid")
				continue
			}

			select {
			case <-ctx.Done():
				return
			case out <- ScanSafeCommitmentsResult{result.SignedCommitment, proof, blockHash, result.Depth, nil}:
			}

		}
	}
}

func makeProof(meta *types.Metadata, api *gsrpc.SubstrateAPI, blockNumber uint32, blockHash types.Hash) (bool, merkle.SimplifiedMMRProof, error) {
	proof1, err := api.RPC.MMR.GenerateProof(blockNumber, blockHash)
	if err != nil {
		return false, merkle.SimplifiedMMRProof{}, fmt.Errorf("mmr_generateProof(%v, %v): %w", blockNumber, blockHash.Hex(), err)
	}

	proof2, err := merkle.ConvertToSimplifiedMMRProof(
		proof1.BlockHash,
		uint64(proof1.Proof.LeafIndex),
		proof1.Leaf,
		uint64(proof1.Proof.LeafCount),
		proof1.Proof.Items,
	)
	if err != nil {
		return false, merkle.SimplifiedMMRProof{}, fmt.Errorf("simplified proof conversion for block %v: %w", proof1.BlockHash.Hex(), err)
	}

	proofIsValid, err := verifyProof(meta, api, proof2)
	if err != nil {
		return false, merkle.SimplifiedMMRProof{}, fmt.Errorf("proof verification: %w", err)
	}

	return proofIsValid, proof2, nil
}

// Verify the actual MMR Root we calculated is same as value in storage of relaychain
func verifyProof(meta *types.Metadata, api *gsrpc.SubstrateAPI, proof merkle.SimplifiedMMRProof) (bool, error) {
	leafEncoded, err := types.EncodeToBytes(proof.Leaf)
	if err != nil {
		return false, err
	}
	leafHashBytes := (&keccak.Keccak256{}).Hash(leafEncoded)

	var leafHash types.H256
	copy(leafHash[:], leafHashBytes[0:32])

	actualRoot := merkle.CalculateMerkleRoot(&proof, leafHash)
	if err != nil {
		return false, err
	}

	var expectedRoot types.H256

	mmrRootKey, err := types.CreateStorageKey(meta, "Mmr", "RootHash", nil, nil)
	if err != nil {
		return false, err
	}

	_, err = api.RPC.State.GetStorage(mmrRootKey, &expectedRoot, types.Hash(proof.Blockhash))
	if err != nil {
		return false, err
	}

	return actualRoot == expectedRoot, nil
}
