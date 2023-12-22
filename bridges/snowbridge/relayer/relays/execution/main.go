package execution

import (
	"context"
	"fmt"
	"math/big"
	"sort"
	"time"

	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	"github.com/sirupsen/logrus"
	log "github.com/sirupsen/logrus"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/snowfork/snowbridge/relayer/chain/parachain"
	"github.com/snowfork/snowbridge/relayer/contracts"
	"github.com/snowfork/snowbridge/relayer/crypto/sr25519"
	"golang.org/x/sync/errgroup"
)

type Relay struct {
	config          *Config
	keypair         *sr25519.Keypair
	paraconn        *parachain.Connection
	ethconn         *ethereum.Connection
	gatewayContract *contracts.Gateway
}

func NewRelay(
	config *Config,
	keypair *sr25519.Keypair,
) *Relay {
	return &Relay{
		config:  config,
		keypair: keypair,
	}
}

func (r *Relay) Start(ctx context.Context, eg *errgroup.Group) error {
	paraconn := parachain.NewConnection(r.config.Sink.Parachain.Endpoint, r.keypair.AsKeyringPair())
	ethconn := ethereum.NewConnection(&r.config.Source.Ethereum, nil)

	err := paraconn.Connect(ctx)
	if err != nil {
		return err
	}
	r.paraconn = paraconn

	err = ethconn.Connect(ctx)
	if err != nil {
		return err
	}
	r.ethconn = ethconn

	writer := parachain.NewParachainWriter(
		paraconn,
		r.config.Sink.Parachain.MaxWatchedExtrinsics,
		r.config.Sink.Parachain.MaxBatchCallSize,
	)

	err = writer.Start(ctx, eg)
	if err != nil {
		return err
	}

	headerCache, err := ethereum.NewHeaderBlockCache(
		&ethereum.DefaultBlockLoader{Conn: ethconn},
	)
	if err != nil {
		return err
	}

	address := common.HexToAddress(r.config.Source.Contracts.Gateway)
	contract, err := contracts.NewGateway(address, ethconn.Client())
	if err != nil {
		return err
	}
	r.gatewayContract = contract

	for {
		select {
		case <-ctx.Done():
			return nil
		case <-time.After(12 * time.Second):
			log.WithFields(log.Fields{
				"channelId": r.config.Source.ChannelID,
			}).Info("Polling Nonces")

			executionHeaderState, err := writer.GetLastExecutionHeaderState()
			if err != nil {
				return err
			}

			paraNonce, err := r.fetchLatestParachainNonce()
			if err != nil {
				return err
			}

			if executionHeaderState.BlockNumber == 0 {
				log.WithFields(log.Fields{
					"channelId": r.config.Source.ChannelID,
				}).Info("Beacon execution state syncing not started, waiting...")
				continue
			}

			ethNonce, err := r.fetchEthereumNonce(ctx, executionHeaderState.BlockNumber)
			if err != nil {
				return err
			}

			log.WithFields(log.Fields{
				"ethBlockNumber": executionHeaderState.BlockNumber,
				"channelId":      types.H256(r.config.Source.ChannelID).Hex(),
				"paraNonce":      paraNonce,
				"ethNonce":       ethNonce,
			}).Info("Polled Nonces")

			if paraNonce == ethNonce {
				continue
			}

			events, err := r.findEvents(ctx, executionHeaderState.BlockNumber, paraNonce+1)
			if err != nil {
				return fmt.Errorf("find events: %w", err)
			}

			for _, ev := range events {
				inboundMsg, err := r.makeInboundMessage(ctx, headerCache, ev)
				if err != nil {
					return fmt.Errorf("make outgoing message: %w", err)
				}
				logger := log.WithFields(log.Fields{
					"paraNonce":   paraNonce,
					"ethNonce":    ethNonce,
					"msgNonce":    ev.Nonce,
					"address":     ev.Raw.Address.Hex(),
					"blockHash":   ev.Raw.BlockHash.Hex(),
					"blockNumber": ev.Raw.BlockNumber,
					"txHash":      ev.Raw.TxHash.Hex(),
					"channelID":   types.H256(ev.ChannelID).Hex(),
				})

				if ev.Nonce <= paraNonce {
					logger.Warn("inbound message outdated, just skipped")
					continue
				}

				err = writer.WriteToParachainAndWatch(ctx, "EthereumInboundQueue.submit", inboundMsg)
				if err != nil {
					logger.Error("inbound message fail to sent")
					return fmt.Errorf("write to parachain: %w", err)
				}
				paraNonce, _ = r.fetchLatestParachainNonce()
				if paraNonce != ev.Nonce {
					logger.Error("inbound message sent but fail to execute")
					return fmt.Errorf("inbound message fail to execute")
				}
				logger.Info("inbound message executed successfully")
			}
		}
	}
}

func (r *Relay) fetchLatestParachainNonce() (uint64, error) {
	paraID := r.config.Source.ChannelID
	encodedParaID, err := types.EncodeToBytes(r.config.Source.ChannelID)
	if err != nil {
		return 0, err
	}

	paraNonceKey, err := types.CreateStorageKey(r.paraconn.Metadata(), "EthereumInboundQueue", "Nonce", encodedParaID, nil)
	if err != nil {
		return 0, fmt.Errorf("create storage key for EthereumInboundQueue.Nonce(%v): %w",
			paraID, err)
	}
	var paraNonce uint64
	ok, err := r.paraconn.API().RPC.State.GetStorageLatest(paraNonceKey, &paraNonce)
	if err != nil {
		return 0, fmt.Errorf("fetch storage EthereumInboundQueue.Nonce(%v): %w",
			paraID, err)
	}
	if !ok {
		paraNonce = 0
	}

	return paraNonce, nil
}

func (r *Relay) fetchEthereumNonce(ctx context.Context, blockNumber uint64) (uint64, error) {
	opts := bind.CallOpts{
		Pending:     false,
		BlockNumber: new(big.Int).SetUint64(blockNumber),
		Context:     ctx,
	}
	_, ethOutboundNonce, err := r.gatewayContract.ChannelNoncesOf(&opts, r.config.Source.ChannelID)
	if err != nil {
		return 0, fmt.Errorf("fetch Gateway.ChannelNoncesOf(%v): %w", r.config.Source.ChannelID, err)
	}

	return ethOutboundNonce, nil
}

const BlocksPerQuery = 4096

func (r *Relay) findEvents(
	ctx context.Context,
	latestFinalizedBlockNumber uint64,
	start uint64,
) ([]*contracts.GatewayOutboundMessageAccepted, error) {

	channelID := r.config.Source.ChannelID

	var allEvents []*contracts.GatewayOutboundMessageAccepted

	blockNumber := latestFinalizedBlockNumber

	for {
		log.Info("loop")

		var begin uint64
		if blockNumber < BlocksPerQuery {
			begin = 0
		} else {
			begin = blockNumber - BlocksPerQuery
		}

		opts := bind.FilterOpts{
			Start:   begin,
			End:     &blockNumber,
			Context: ctx,
		}

		done, events, err := r.findEventsWithFilter(&opts, channelID, start)
		if err != nil {
			return nil, fmt.Errorf("filter events: %w", err)
		}

		if len(events) > 0 {
			allEvents = append(allEvents, events...)
		}

		blockNumber = begin

		if done || begin == 0 {
			break
		}
	}

	sort.SliceStable(allEvents, func(i, j int) bool {
		return allEvents[i].Nonce < allEvents[j].Nonce
	})

	return allEvents, nil
}

func (r *Relay) findEventsWithFilter(opts *bind.FilterOpts, channelID [32]byte, start uint64) (bool, []*contracts.GatewayOutboundMessageAccepted, error) {
	iter, err := r.gatewayContract.FilterOutboundMessageAccepted(opts, [][32]byte{channelID}, [][32]byte{})
	if err != nil {
		return false, nil, err
	}

	var events []*contracts.GatewayOutboundMessageAccepted
	done := false

	for {
		more := iter.Next()
		if !more {
			err = iter.Error()
			if err != nil {
				return false, nil, err
			}
			break
		}
		if iter.Event.Nonce >= start {
			events = append(events, iter.Event)
		}
		if iter.Event.Nonce == start && opts.Start != 0 {
			done = true
			iter.Close()
			break
		}
	}

	return done, events, nil
}

func (r *Relay) makeInboundMessage(
	ctx context.Context,
	headerCache *ethereum.HeaderCache,
	event *contracts.GatewayOutboundMessageAccepted,
) (*parachain.Message, error) {
	receiptTrie, err := headerCache.GetReceiptTrie(ctx, event.Raw.BlockHash)
	if err != nil {
		log.WithFields(logrus.Fields{
			"blockHash":   event.Raw.BlockHash.Hex(),
			"blockNumber": event.Raw.BlockNumber,
			"txHash":      event.Raw.TxHash.Hex(),
		}).WithError(err).Error("Failed to get receipt trie for event")
		return nil, err
	}

	msg, err := ethereum.MakeMessageFromEvent(&event.Raw, receiptTrie)
	if err != nil {
		log.WithFields(logrus.Fields{
			"address":     event.Raw.Address.Hex(),
			"blockHash":   event.Raw.BlockHash.Hex(),
			"blockNumber": event.Raw.BlockNumber,
			"txHash":      event.Raw.TxHash.Hex(),
		}).WithError(err).Error("Failed to generate message from ethereum event")
		return nil, err
	}

	return msg, nil
}
