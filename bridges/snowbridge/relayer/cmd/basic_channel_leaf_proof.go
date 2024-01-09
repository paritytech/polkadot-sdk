package cmd

import (
	"fmt"

	gsrpc "github.com/snowfork/go-substrate-rpc-client/v4"

	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/spf13/cobra"
)

func basicChannelLeafProofCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "basic-channel-leaf-proof",
		Short: "fetch proof for leaf",
		Args:  cobra.ExactArgs(0),
		RunE:  BasicChannelLeafProofFn,
	}

	cmd.Flags().StringP("url", "u", "", "Parachain URL")
	cmd.MarkFlagRequired("url")

	cmd.Flags().BytesHex(
		"commitment-hash",
		[]byte{},
		"Commitment Hash",
	)

	cmd.Flags().Uint64(
		"leaf-index",
		1,
		"Leaf index",
	)

	return cmd
}

func BasicChannelLeafProofFn(cmd *cobra.Command, _ []string) error {
	url, _ := cmd.Flags().GetString("url")
	commitmentHashHex, _ := cmd.Flags().GetBytesHex("commitment-hash")
	leafIndex, _ := cmd.Flags().GetUint64("leaf-index")

	var commitmentHash types.Hash
	copy(commitmentHash[:], commitmentHashHex[0:32])

	api, err := gsrpc.NewSubstrateAPI(url)
	if err != nil {
		return fmt.Errorf("create client: %w", err)
	}

	var proofHex string
	err = api.Client.Call(&proofHex, "outboundQueue_getMerkleProof", commitmentHash.Hex(), leafIndex)
	if err != nil {
		return fmt.Errorf("call rpc: %w", err)
	}

	var proof MerkleProof
	err = types.DecodeFromHexString(proofHex, &proof)
	if err != nil {
		return fmt.Errorf("decode: %w", err)
	}

	fmt.Printf("%#+v", proof)

	return nil
}

type MerkleProof struct {
	Root           types.H256
	Proof          []types.H256
	NumberOfLeaves uint64
	LeafIndex      uint64
	Leaf           []byte
}
