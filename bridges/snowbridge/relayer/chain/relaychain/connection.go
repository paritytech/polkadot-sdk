// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package relaychain

import (
	"context"
	"fmt"

	gsrpc "github.com/snowfork/go-substrate-rpc-client/v4"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"

	log "github.com/sirupsen/logrus"
)

type Connection struct {
	endpoint    string
	api         *gsrpc.SubstrateAPI
	metadata    types.Metadata
	genesisHash types.Hash
}

func NewConnection(endpoint string) *Connection {
	return &Connection{
		endpoint: endpoint,
	}
}

func (co *Connection) API() *gsrpc.SubstrateAPI {
	return co.api
}

func (co *Connection) Metadata() *types.Metadata {
	return &co.metadata
}

func (co *Connection) Connect(_ context.Context) error {
	// Initialize API
	api, err := gsrpc.NewSubstrateAPI(co.endpoint)
	if err != nil {
		return err
	}
	co.api = api

	// Fetch metadata
	meta, err := api.RPC.State.GetMetadataLatest()
	if err != nil {
		return err
	}
	co.metadata = *meta

	// Fetch genesis hash
	genesisHash, err := api.RPC.Chain.GetBlockHash(0)
	if err != nil {
		return err
	}
	co.genesisHash = genesisHash

	log.WithFields(log.Fields{
		"endpoint":    co.endpoint,
		"metaVersion": meta.Version,
	}).Info("Connected to chain")

	return nil
}

func (co *Connection) Close() {
	// TODO: Fix design issue in GSRPC preventing on-demand closing of connections
}

func (conn *Connection) GetMMRRootHash(blockHash types.Hash) (types.Hash, error) {
	mmrRootHashKey, err := types.CreateStorageKey(conn.Metadata(), "Mmr", "RootHash", nil, nil)
	if err != nil {
		return types.Hash{}, fmt.Errorf("create storage key: %w", err)
	}
	var mmrRootHash types.Hash
	ok, err := conn.API().RPC.State.GetStorage(mmrRootHashKey, &mmrRootHash, blockHash)
	if err != nil {
		return types.Hash{}, fmt.Errorf("query storage for mmr root hash at block %v: %w", blockHash.Hex(), err)
	}
	if !ok {
		return types.Hash{}, fmt.Errorf("Mmr.RootHash storage item does not exist")
	}
	return mmrRootHash, nil
}

func (co *Connection) GenerateProofForBlock(
	blockNumber uint64,
	latestBeefyBlockHash types.Hash,
) (types.GenerateMMRProofResponse, error) {
	log.WithFields(log.Fields{
		"blockNumber": blockNumber,
		"blockHash":   latestBeefyBlockHash.Hex(),
	}).Debug("Getting MMR Leaf for block...")

	proofResponse, err := co.API().RPC.MMR.GenerateProof(uint32(blockNumber), latestBeefyBlockHash)
	if err != nil {
		return types.GenerateMMRProofResponse{}, err
	}

	var proofItemsHex = []string{}
	for _, item := range proofResponse.Proof.Items {
		proofItemsHex = append(proofItemsHex, item.Hex())
	}

	log.WithFields(log.Fields{
		"BlockHash": proofResponse.BlockHash.Hex(),
		"Leaf": log.Fields{
			"ParentNumber":   proofResponse.Leaf.ParentNumberAndHash.ParentNumber,
			"ParentHash":     proofResponse.Leaf.ParentNumberAndHash.Hash.Hex(),
			"ParachainHeads": proofResponse.Leaf.ParachainHeads.Hex(),
			"NextAuthoritySet": log.Fields{
				"Id":   proofResponse.Leaf.BeefyNextAuthoritySet.ID,
				"Len":  proofResponse.Leaf.BeefyNextAuthoritySet.Len,
				"Root": proofResponse.Leaf.BeefyNextAuthoritySet.Root.Hex(),
			},
		},
		"Proof": log.Fields{
			"LeafIndex": proofResponse.Proof.LeafIndex,
			"LeafCount": proofResponse.Proof.LeafCount,
			"Items":     proofItemsHex,
		},
	}).Debug("Generated MMR proof")

	return proofResponse, nil
}

type ParaHead struct {
	ParaID uint32
	Data   types.Bytes
}

// Fetches heads for each parachain Id filtering out para threads.
func (conn *Connection) FetchParachainHeads(relayChainBlockHash types.Hash) ([]ParaHead, error) {
	// Fetch para heads
	paraHeads, err := conn.fetchParaHeads(relayChainBlockHash)
	if err != nil {
		log.WithError(err).Error("Cannot fetch para heads.")
		return nil, err
	}

	// fetch ids of parachains (not including parathreads)
	var parachainIDs []uint32
	parachainsKey, err := types.CreateStorageKey(conn.Metadata(), "Paras", "Parachains", nil, nil)
	if err != nil {
		return nil, err
	}

	_, err = conn.API().RPC.State.GetStorage(parachainsKey, &parachainIDs, relayChainBlockHash)
	if err != nil {
		return nil, err
	}

	// filter out parathreads
	var parachainHeads []ParaHead
	for _, v := range parachainIDs {
		if head, ok := paraHeads[v]; ok {
			parachainHeads = append(parachainHeads, head)
		}
	}
	return parachainHeads, nil
}

func (co *Connection) FetchParachainHead(relayBlockhash types.Hash, paraID uint32, header *types.Header) (bool, error) {
	encodedParaID, err := types.EncodeToBytes(paraID)
	if err != nil {
		return false, err
	}

	storageKey, err := types.CreateStorageKey(co.Metadata(), "Paras", "Heads", encodedParaID, nil)
	if err != nil {
		return false, err
	}

	var headerBytes types.Bytes
	ok, err := co.API().RPC.State.GetStorage(storageKey, &headerBytes, relayBlockhash)
	if err != nil {
		return false, err
	}

	if !ok {
		return false, nil
	}

	if err := types.DecodeFromBytes(headerBytes, header); err != nil {
		return false, err
	}

	return true, nil
}

func (co *Connection) IsParachainRegistered(relayBlockHash types.Hash, paraID uint32) (bool, error) {
	var header types.Header
	ok, err := co.FetchParachainHead(relayBlockHash, paraID, &header)
	if err != nil {
		return false, fmt.Errorf("fetch parachain header: %w", err)
	}
	return ok, nil
}

func (co *Connection) FetchMMRLeafCount(relayBlockhash types.Hash) (uint64, error) {
	mmrLeafCountKey, err := types.CreateStorageKey(co.Metadata(), "Mmr", "NumberOfLeaves", nil, nil)
	if err != nil {
		return 0, err
	}
	var mmrLeafCount uint64

	ok, err := co.API().RPC.State.GetStorage(mmrLeafCountKey, &mmrLeafCount, relayBlockhash)
	if err != nil {
		return 0, err
	}

	if !ok {
		return 0, fmt.Errorf("MMR Leaf Count Not Found")
	}

	log.WithFields(log.Fields{
		"mmrLeafCount": mmrLeafCount,
	}).Info("MMR Leaf Count")

	return mmrLeafCount, nil
}

func (co *Connection) fetchKeys(keyPrefix []byte, blockHash types.Hash) ([]types.StorageKey, error) {
	const pageSize = 200
	var startKey *types.StorageKey

	if pageSize < 1 {
		return nil, fmt.Errorf("page size cannot be zero")
	}

	var results []types.StorageKey
	log.WithFields(log.Fields{
		"keyPrefix": keyPrefix,
		"blockHash": blockHash.Hex(),
		"pageSize":  pageSize,
	}).Trace("Fetching paged keys.")

	pageIndex := 0
	for {
		response, err := co.API().RPC.State.GetKeysPaged(keyPrefix, pageSize, startKey, blockHash)
		if err != nil {
			return nil, err
		}

		log.WithFields(log.Fields{
			"keysInPage": len(response),
			"pageIndex":  pageIndex,
		}).Trace("Fetched a page of keys.")

		results = append(results, response...)
		if uint32(len(response)) < pageSize {
			break
		} else {
			startKey = &response[len(response)-1]
			pageIndex++
		}
	}

	log.WithFields(log.Fields{
		"totalNumKeys":  len(results),
		"totalNumPages": pageIndex + 1,
	}).Trace("Fetching of paged keys complete.")

	return results, nil
}

// Offset of encoded para id in storage key.
// The key is of this format:
//
//	ParaId: u32
//	Key: hash_twox_128("Paras") + hash_twox_128("Heads") + hash_twox_64(ParaId) + Encode(ParaId)
const ParaIDOffset = 16 + 16 + 8

func (co *Connection) fetchParaHeads(blockHash types.Hash) (map[uint32]ParaHead, error) {
	keyPrefix := types.CreateStorageKeyPrefix("Paras", "Heads")
	keys, err := co.fetchKeys(keyPrefix, blockHash)
	if err != nil {
		log.WithError(err).Error("Failed to get all parachain keys")
		return nil, err
	}

	log.WithFields(log.Fields{
		"numKeys":          len(keys),
		"storageKeyPrefix": fmt.Sprintf("%#x", keyPrefix),
		"block":            blockHash.Hex(),
	}).Trace("Found keys for Paras.Heads storage map")

	changeSets, err := co.API().RPC.State.QueryStorageAt(keys, blockHash)
	if err != nil {
		log.WithError(err).Error("Failed to get all parachain headers")
		return nil, err
	}

	heads := make(map[uint32]ParaHead)
	for _, changeSet := range changeSets {
		for _, change := range changeSet.Changes {
			if change.StorageData.IsNone() {
				continue
			}

			var paraID uint32
			if err := types.DecodeFromBytes(change.StorageKey[ParaIDOffset:], &paraID); err != nil {
				log.WithError(err).Error("Failed to decode parachain ID")
				return nil, err
			}

			_, headDataWrapped := change.StorageData.Unwrap()

			var headData types.Bytes
			if err := types.DecodeFromBytes(headDataWrapped, &headData); err != nil {
				log.WithError(err).Error("Failed to decode HeadData wrapper")
				return nil, err
			}

			heads[paraID] = ParaHead{
				ParaID: paraID,
				Data:   headData,
			}
		}
	}

	return heads, nil
}
