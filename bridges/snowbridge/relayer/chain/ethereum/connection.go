// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package ethereum

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"time"

	"github.com/ethereum/go-ethereum"
	goEthereum "github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/sirupsen/logrus"

	"github.com/snowfork/snowbridge/relayer/config"
	"github.com/snowfork/snowbridge/relayer/crypto/secp256k1"

	log "github.com/sirupsen/logrus"
)

type Connection struct {
	endpoint string
	kp       *secp256k1.Keypair
	client   *ethclient.Client
	chainID  *big.Int
	config   *config.EthereumConfig
}

type JsonError interface {
	Error() string
	ErrorCode() int
	ErrorData() interface{}
}

func NewConnection(config *config.EthereumConfig, kp *secp256k1.Keypair) *Connection {
	return &Connection{
		endpoint: config.Endpoint,
		kp:       kp,
		config:   config,
	}
}

func (co *Connection) Connect(ctx context.Context) error {
	client, err := ethclient.Dial(co.endpoint)
	if err != nil {
		return err
	}

	chainID, err := client.NetworkID(ctx)
	if err != nil {
		return err
	}

	log.WithFields(logrus.Fields{
		"endpoint": co.endpoint,
		"chainID":  chainID,
	}).Info("Connected to chain")

	co.client = client
	co.chainID = chainID

	return nil
}

func (co *Connection) Close() {
	if co.client != nil {
		co.client.Close()
	}
}

func (co *Connection) Client() *ethclient.Client {
	return co.client
}

func (co *Connection) Keypair() *secp256k1.Keypair {
	return co.kp
}

func (co *Connection) ChainID() *big.Int {
	return co.chainID
}

func (co *Connection) queryFailingError(ctx context.Context, hash common.Hash) error {
	tx, _, err := co.client.TransactionByHash(ctx, hash)
	if err != nil {
		return err
	}

	from, err := types.Sender(types.LatestSignerForChainID(tx.ChainId()), tx)
	if err != nil {
		return err
	}

	params := ethereum.CallMsg{
		From:     from,
		To:       tx.To(),
		Gas:      tx.Gas(),
		GasPrice: tx.GasPrice(),
		Value:    tx.Value(),
		Data:     tx.Data(),
	}

	log.WithFields(logrus.Fields{
		"From":     from,
		"To":       tx.To(),
		"Gas":      tx.Gas(),
		"GasPrice": tx.GasPrice(),
		"Value":    tx.Value(),
		"Data":     hex.EncodeToString(tx.Data()),
	}).Info("Call info")

	// The logger does a test call to the actual contract to check for any revert message and log it, as well
	// as logging the call info. This is because the golang client can sometimes suppress the log message and so
	// it can be helpful to use the call info to do the same call in Truffle/Web3js to get better logs.
	_, err = co.client.CallContract(ctx, params, nil)
	if err != nil {
		return err
	}
	return nil
}

func (co *Connection) waitForTransaction(ctx context.Context, tx *types.Transaction, confirmations uint64) (*types.Receipt, error) {
	for {
		receipt, err := co.pollTransaction(ctx, tx, confirmations)
		if err != nil {
			return nil, err
		}

		if receipt != nil {
			return receipt, nil
		}

		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(500 * time.Millisecond):
		}
	}
}

func (co *Connection) pollTransaction(ctx context.Context, tx *types.Transaction, confirmations uint64) (*types.Receipt, error) {
	receipt, err := co.Client().TransactionReceipt(ctx, tx.Hash())
	if err != nil {
		if errors.Is(err, goEthereum.NotFound) {
			return nil, nil
		}
	}

	latestHeader, err := co.Client().HeaderByNumber(ctx, nil)
	if err != nil {
		return nil, err
	}

	if latestHeader.Number.Uint64()-receipt.BlockNumber.Uint64() >= confirmations {
		return receipt, nil
	}

	return nil, nil
}

func (co *Connection) WatchTransaction(ctx context.Context, tx *types.Transaction, confirmations uint64) (*types.Receipt, error) {
	receipt, err := co.waitForTransaction(ctx, tx, confirmations)
	if err != nil {
		return nil, err
	}
	if receipt.Status != 1 {
		err = co.queryFailingError(ctx, receipt.TxHash)
		logFields := log.Fields{
			"txHash": tx.Hash().Hex(),
		}
		if err != nil {
			logFields["error"] = err.Error()
			jsonErr, ok := err.(JsonError)
			if ok {
				errorCode := fmt.Sprintf("%v", jsonErr.ErrorData())
				logFields["code"] = errorCode
			}
		}
		log.WithFields(logFields).Error("Failed to send transaction")
		return receipt, err
	}
	return receipt, nil
}

func (co *Connection) MakeTxOpts(ctx context.Context) *bind.TransactOpts {
	chainID := co.ChainID()
	keypair := co.Keypair()

	options := bind.TransactOpts{
		From: keypair.CommonAddress(),
		Signer: func(_ common.Address, tx *types.Transaction) (*types.Transaction, error) {
			return types.SignTx(tx, types.LatestSignerForChainID(chainID), keypair.PrivateKey())
		},
		Context: ctx,
	}

	if co.config.GasFeeCap > 0 {
		fee := big.NewInt(0)
		fee.SetUint64(co.config.GasFeeCap)
		options.GasFeeCap = fee
	}

	if co.config.GasTipCap > 0 {
		tip := big.NewInt(0)
		tip.SetUint64(co.config.GasTipCap)
		options.GasTipCap = tip
	}

	if co.config.GasLimit > 0 {
		options.GasLimit = co.config.GasLimit
	}

	return &options
}
