package cmd

import (
	"errors"
	"fmt"

	log "github.com/sirupsen/logrus"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/chain/relaychain"
	"github.com/snowfork/snowbridge/relayer/relays/parachain"
	"github.com/spf13/cobra"
)

func parachainHeadProofCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "parachain-head-proof",
		Short: "Prove a block using beefy.",
		Args:  cobra.ExactArgs(0),
		RunE:  ParachainHeadProofFn,
	}

	cmd.Flags().StringP("url", "u", "", "Polkadot URL")
	cmd.MarkFlagRequired("url")

	cmd.Flags().BytesHex(
		"beefy-block-hash",
		[]byte{},
		"Latest block finalized by BEEFY",
	)
	cmd.MarkFlagRequired("beefy-block-hash")

	cmd.Flags().Uint64(
		"relaychain-block",
		0,
		"The relaychain block in which the parachain header was was accepted.",
	)
	cmd.MarkFlagRequired("relaychain-block")

	cmd.Flags().Uint32(
		"parachain-id",
		0,
		"The parachain id for the block you are trying to prove.",
	)
	cmd.MarkFlagRequired("parachain-id")

	cmd.Flags().Uint64(
		"parachain-block",
		0,
		"The parachain block you are trying to prove. i.e. The block containing the message.",
	)
	cmd.MarkFlagRequired("parachain-block")
	return cmd
}

func ParachainHeadProofFn(cmd *cobra.Command, _ []string) error {
	ctx := cmd.Context()

	url, _ := cmd.Flags().GetString("url")
	conn := relaychain.NewConnection(url)
	err := conn.Connect(ctx)
	if err != nil {
		log.WithError(err).Error("Cannot connect.")
		return err
	}

	beefyBlockHashHex, _ := cmd.Flags().GetBytesHex("beefy-block-hash")
	if len(beefyBlockHashHex) != 32 {
		log.Error("Incorrect length of beefy block hash.")
		return errors.New("incorrect block hash length")
	}
	var beefyBlockHash types.Hash
	copy(beefyBlockHash[:], beefyBlockHashHex[0:32])

	relayChainBlock, _ := cmd.Flags().GetUint64("relaychain-block")
	mmrProof, err := conn.GenerateProofForBlock(relayChainBlock, beefyBlockHash)
	if err != nil {
		log.WithError(err).Error("Cannot connect.")
		return err
	}
	log.WithFields(log.Fields{
		"relayChainBlock": relayChainBlock,
		"beefyBlockHash":  beefyBlockHash,
		"mmrProof":        mmrProof,
	}).Info("conn.GenerateProofForBlock")

	paraID, _ := cmd.Flags().GetUint32("parachain-id")
	parachainBlock, _ := cmd.Flags().GetUint64("parachain-block")

	relayChainBlockHash, err := conn.API().RPC.Chain.GetBlockHash(relayChainBlock)
	if err != nil {
		log.WithError(err).Error("Cannot fetch parachain block hash.")
		return err
	}

	paraHeadsAsSlice, err := conn.FetchParachainHeads(relayChainBlockHash)
	if err != nil {
		log.WithError(err).Error("Cannot fetch parachain headers")
		return err
	}

	var parachainHeader types.Header
	ok, err := conn.FetchParachainHead(relayChainBlockHash, paraID, &parachainHeader)
	if err != nil {
		log.WithError(err).Error("Cannot fetch our parachain header")
		return err
	}

	if !ok {
		log.WithError(err).Error("parachain is not registered")
		return fmt.Errorf("parachain is not registered")
	}

	log.WithFields(log.Fields{
		"paraHeadsAsSlice":    paraHeadsAsSlice,
		"parachainHeader":     parachainHeader,
		"paraId":              paraID,
		"relayChainBlockHash": relayChainBlockHash.Hex(),
	}).Info("parachain.CreateParachainMerkleProof")

	merkleProofData, err := parachain.CreateParachainMerkleProof(paraHeadsAsSlice, paraID)
	if err != nil {
		log.WithError(err).Error("Cannot create merkle proof.")
		return err
	}
	log.WithFields(log.Fields{
		"paraHeadsAsSlice": paraHeadsAsSlice,
		"paraId":           paraID,
		"merkleProofData":  merkleProofData,
	}).Info("parachain.CreateParachainMerkleProof")

	log.WithFields(log.Fields{
		"parachainId":           paraID,
		"relaychainBlockHash":   relayChainBlockHash.Hex(),
		"relaychainBlockNumber": relayChainBlock,
		"parachainBlockNumber":  parachainBlock,
		"paraHeads":             paraHeadsAsSlice,
		"parachainHeader":       parachainHeader,
	}).Info("Generated proof input for parachain block.")

	log.WithFields(log.Fields{
		"mmrProofParachainHeads":           mmrProof.Leaf.ParachainHeads.Hex(),
		"mmrProofParentNumberAndHash":      mmrProof.Leaf.ParentNumberAndHash,
		"computedProofParachainHeads":      merkleProofData.Root.Hex(),
		"computedProofParentNumberAndHash": types.ParentNumberAndHash{ParentNumber: types.U32(relayChainBlock), Hash: relayChainBlockHash},
	}).Info("Complete.")

	return nil
}
