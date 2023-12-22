// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only
//go:build exclude

package cmd

import (
	"context"
	"encoding/hex"
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"

	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	gethCommon "github.com/ethereum/go-ethereum/common"
	gethTypes "github.com/ethereum/go-ethereum/core/types"
	gethTrie "github.com/ethereum/go-ethereum/trie"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/snowfork/snowbridge/relayer/chain/parachain"
	"github.com/snowfork/snowbridge/relayer/contracts"
)

func fetchMessagesCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "fetch-messages",
		Short:   "Retrieve the messages specified by block and index",
		Args:    cobra.ExactArgs(0),
		Example: "snowbridge-relay fetch-messages -b 812e7d414071648252bb3c2dc9c6d2f292fb615634606f9251191c7372eb4acc -i 123",
		RunE:    fetchMessagesFunc,
	}

	cmd.Flags().StringP("url", "u", "", "Ethereum URL")
	cmd.MarkFlagRequired("url")

	cmd.Flags().String("bo-channel", "", "Address of basic outbound channel")
	cmd.MarkFlagRequired("bo-channel")

	cmd.Flags().StringP("block", "b", "", "Block hash")
	cmd.Flags().Uint64P(
		"index",
		"i",
		0,
		"Index in the block of the receipt (or transaction) that contains the event",
	)
	cmd.MarkFlagRequired("block")

	viper.BindPFlags(cmd.Flags())

	return cmd
}

func fetchMessagesFunc(_ *cobra.Command, _ []string) error {
	hashBytes, err := hex.DecodeString(viper.GetString("block"))
	if err != nil {
		return err
	}
	blockHash := gethCommon.BytesToHash(hashBytes)
	index := viper.GetUint64("index")
	if err != nil {
		return err
	}

	contractEvents, trie, err := getEthContractEventsAndTrie(blockHash, index)
	if err != nil {
		return err
	}

	for _, event := range contractEvents {
		printEthContractEventForSub(event, trie)
	}
	return nil
}

func getEthContractEventsAndTrie(
	blockHash gethCommon.Hash,
	index uint64,
) ([]*gethTypes.Log, *gethTrie.Trie, error) {
	ctx := context.Background()

	conn := ethereum.NewConnection(viper.GetString("url"), nil)
	err := conn.Connect(ctx)
	if err != nil {
		return nil, nil, err
	}
	defer conn.Close()

	var address common.Address

	address = common.HexToAddress(viper.GetString("bo-channel"))
	outboundQueue, err := contracts.NewOutboundQueue(address, conn.Client())
	if err != nil {
		return nil, nil, err
	}

	loader := ethereum.DefaultBlockLoader{Conn: conn}
	block, err := loader.GetBlock(ctx, blockHash)
	if err != nil {
		return nil, nil, err
	}

	receipts, err := loader.GetAllReceipts(ctx, block)
	if err != nil {
		return nil, nil, err
	}

	trie, err := ethereum.MakeTrie(receipts)
	if err != nil {
		return nil, nil, err
	}

	basicEvents, err := getEthBasicMessages(ctx, outboundQueue, block.NumberU64(), index)
	if err != nil {
		return nil, nil, err
	}

	return basicEvents, trie, nil
}

func getEthBasicMessages(
	ctx context.Context,
	contract *contracts.OutboundQueue,
	blockNumber uint64,
	index uint64,
) ([]*gethTypes.Log, error) {
	events := make([]*gethTypes.Log, 0)
	filterOps := bind.FilterOpts{Start: blockNumber, End: &blockNumber, Context: ctx}

	iter, err := contract.FilterMessage(&filterOps)
	if err != nil {
		return nil, err
	}

	for {
		more := iter.Next()
		if !more {
			err = iter.Error()
			if err != nil {
				return nil, err
			}
			break
		}

		if uint64(iter.Event.Raw.TxIndex) != index {
			continue
		}
		events = append(events, &iter.Event.Raw)
	}

	return events, nil
}

func printEthContractEventForSub(event *gethTypes.Log, trie *gethTrie.Trie) error {
	message, err := ethereum.MakeMessageFromEvent(event, trie)
	if err != nil {
		return err
	}

	msgInner, ok := message.Args[0].(parachain.Message)
	if !ok {
		return err
	}

	formatProofVec := func(data []types.Bytes) string {
		hexRep := make([]string, len(data))
		for i, datum := range data {
			hexRep[i] = fmt.Sprintf("hex!(\"%s\").to_vec()", hex.EncodeToString(datum))
		}
		return fmt.Sprintf(`vec![
			%s,
		]`, strings.Join(hexRep, ",\n"))
	}

	fmt.Println("")
	fmt.Printf(
		`Message {
			data: hex!("%s").to_vec(),
			proof: Proof {
				block_hash: hex!("%x").into(),
				tx_index: %d,
				data: (
					%s,
					%s,
				),
			},
		}`,
		hex.EncodeToString(msgInner.Data),
		msgInner.Proof.BlockHash,
		msgInner.Proof.TxIndex,
		formatProofVec(msgInner.Proof.Data.Keys),
		formatProofVec(msgInner.Proof.Data.Values),
	)
	fmt.Println("")
	return nil
}
