// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package cmd

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"

	gethCommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/spf13/cobra"

	gethTypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
)

type Format string

const (
	RustFmt Format = "rust"
	JSONFmt Format = "json"
)

func getBlockCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "dump-block",
		Short:   "Retrieve a block, either specified by hash or the latest finalized block",
		Args:    cobra.ExactArgs(0),
		Example: "snowbridge-relay dump-block",
		RunE:    GetBlockFn,
	}
	cmd.Flags().StringP("block", "b", "", "Block hash")
	cmd.Flags().StringP("url", "u", "", "Ethereum endpoint")
	cmd.Flags().StringP("descendants-until-final", "d", "", "Descendants until finals")

	cmd.Flags().StringP(
		"format",
		"f",
		"json",
		"The output format. Options are 'rust' and 'json'. They correspond to the Substrate genesis config formats.",
	)
	cmd.Flags().BoolP(
		"include-proof",
		"i",
		false,
		"Whether to also retrieve the header's PoW Merkle proofs. The output is SCALE-encoded. It will take several minutes to generate the DAG if it hasn't been cached.",
	)
	return cmd
}

func GetBlockFn(cmd *cobra.Command, _ []string) error {
	hashStr := cmd.Flags().Lookup("block").Value.String()
	var blockHash *gethCommon.Hash
	if len(hashStr) > 0 {
		hashBytes, err := hex.DecodeString(hashStr)
		if err != nil {
			return err
		}
		hash := gethCommon.BytesToHash(hashBytes)
		blockHash = &hash
	}
	format := Format(cmd.Flags().Lookup("format").Value.String())

	url := cmd.Flags().Lookup("url").Value.String()
	header, err := getEthBlock(url, blockHash)
	if err != nil {
		return err
	}

	err = printEthBlockForSub(header, format)
	if err != nil {
		return err
	}

	return nil
}

func getEthBlock(url string, blockHash *gethCommon.Hash) (*gethTypes.Header, error) {
	ctx := context.Background()
	client, err := ethclient.Dial(url)
	if err != nil {
		return nil, err
	}
	defer client.Close()

	var header *gethTypes.Header
	if blockHash == nil {
		header, err = client.HeaderByNumber(ctx, nil)
		if err != nil {
			return nil, err
		}
	} else {
		header, err = client.HeaderByHash(ctx, *blockHash)
		if err != nil {
			return nil, err
		}
	}

	return header, nil
}

func printEthBlockForSub(header *gethTypes.Header, format Format) error {
	headerForSub, err := ethereum.MakeHeaderData(header)
	if err != nil {
		return err
	}

	if format == RustFmt {
		fmt.Printf(
			`EthereumHeader {
	parent_hash: hex!("%x").into(),
	timestamp: %du64.into(),
	number: %du64.into(),
	author: hex!("%x").into(),
	transactions_root: hex!("%x").into(),
	ommers_hash: hex!("%x").into(),
	extra_data: hex!("%x").into(),
	state_root: hex!("%x").into(),
	receipts_root: hex!("%x").into(),
	logs_bloom: (&hex!("%x")).into(),
	gas_used: %du64.into(),
	gas_limit: %du64.into(),
	difficulty: %du64.into(),
	seal: vec![
		hex!("%x").to_vec(),
		hex!("%x").to_vec(),
	],
}
`,
			headerForSub.Fields.ParentHash,
			header.Time,
			headerForSub.Fields.Number,
			headerForSub.Fields.Author,
			headerForSub.Fields.TransactionsRoot,
			headerForSub.Fields.OmmersHash,
			headerForSub.Fields.ExtraData,
			headerForSub.Fields.StateRoot,
			headerForSub.Fields.ReceiptsRoot,
			headerForSub.Fields.LogsBloom,
			headerForSub.Fields.GasUsed,
			headerForSub.Fields.GasLimit,
			headerForSub.Fields.Difficulty,
			headerForSub.Fields.Seal[0],
			headerForSub.Fields.Seal[1],
		)
		fmt.Println("")
	} else {
		extraData, err := json.Marshal(bytesAsArray64(headerForSub.Fields.ExtraData))
		if err != nil {
			return err
		}
		logsBloom, err := json.Marshal(headerForSub.Fields.LogsBloom)
		if err != nil {
			return err
		}
		seal1, err := json.Marshal(bytesAsArray64(headerForSub.Fields.Seal[0]))
		if err != nil {
			return err
		}
		seal2, err := json.Marshal(bytesAsArray64(headerForSub.Fields.Seal[1]))
		if err != nil {
			return err
		}

		fmt.Printf(
			`{
  "parent_hash": "%s",
  "timestamp": %d,
  "number": %d,
  "author": "%s",
  "transactions_root": "%s",
  "ommers_hash": "%s",
  "extra_data": %s,
  "state_root": "%s",
  "receipts_root": "%s",
  "logs_bloom": %s,
  "gas_used": "%#x",
  "gas_limit": "%#x",
  "difficulty": "%#x",
  "seal": [
    %s,
    %s
  ]
}`,
			headerForSub.Fields.ParentHash.Hex(),
			header.Time,
			headerForSub.Fields.Number,
			headerForSub.Fields.Author.Hex(),
			headerForSub.Fields.TransactionsRoot.Hex(),
			headerForSub.Fields.OmmersHash.Hex(),
			extraData,
			headerForSub.Fields.StateRoot.Hex(),
			headerForSub.Fields.ReceiptsRoot.Hex(),
			logsBloom,
			headerForSub.Fields.GasUsed,
			headerForSub.Fields.GasLimit,
			headerForSub.Fields.Difficulty,
			seal1,
			seal2,
		)
		fmt.Println("")
	}

	return nil
}

func bytesAsArray64(bytes []byte) []uint64 {
	arr := make([]uint64, len(bytes))
	for i, v := range bytes {
		arr[i] = uint64(v)
	}
	return arr
}
