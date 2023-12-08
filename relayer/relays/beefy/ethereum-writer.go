package beefy

import (
	"context"
	"encoding/hex"
	"fmt"
	"math/big"
	"math/rand"

	"golang.org/x/sync/errgroup"

	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"

	"github.com/sirupsen/logrus"

	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/snowfork/snowbridge/relayer/contracts"
	"github.com/snowfork/snowbridge/relayer/relays/beefy/bitfield"

	log "github.com/sirupsen/logrus"
)

type EthereumWriter struct {
	config          *SinkConfig
	conn            *ethereum.Connection
	contract        *contracts.BeefyClient
	blockWaitPeriod uint64
}

func NewEthereumWriter(
	config *SinkConfig,
	conn *ethereum.Connection,
) *EthereumWriter {
	return &EthereumWriter{
		config: config,
		conn:   conn,
	}
}

func (wr *EthereumWriter) Start(ctx context.Context, eg *errgroup.Group, requests <-chan Request) error {
	address := common.HexToAddress(wr.config.Contracts.BeefyClient)
	contract, err := contracts.NewBeefyClient(address, wr.conn.Client())
	if err != nil {
		return fmt.Errorf("create beefy client: %w", err)
	}
	wr.contract = contract

	callOpts := bind.CallOpts{
		Context: ctx,
	}
	blockWaitPeriod, err := wr.contract.RandaoCommitDelay(&callOpts)
	if err != nil {
		return fmt.Errorf("create randao commit delay: %w", err)
	}
	wr.blockWaitPeriod = blockWaitPeriod.Uint64()
	log.WithField("randaoCommitDelay", wr.blockWaitPeriod).Trace("Fetched randaoCommitDelay")

	// launch task processor
	eg.Go(func() error {
		for {
			select {
			case <-ctx.Done():
				return nil
			case task, ok := <-requests:
				if !ok {
					return nil
				}

				err := wr.submit(ctx, task)
				if err != nil {
					return fmt.Errorf("submit request: %w", err)
				}
			}
		}
	})

	return nil
}

func (wr *EthereumWriter) submit(ctx context.Context, task Request) error {
	// Initial submission
	tx, initialBitfield, err := wr.doSubmitInitial(ctx, &task)
	if err != nil {
		log.WithError(err).Error("Failed to send initial signature commitment")
		return err
	}

	// Wait RandaoCommitDelay before submit CommitPrevRandao to prevent attacker from manipulating committee memberships
	// Details in https://eth2book.info/altair/part3/config/preset/#max_seed_lookahead
	_, err = wr.conn.WatchTransaction(ctx, tx, wr.blockWaitPeriod+1)
	if err != nil {
		log.WithError(err).Error("Failed to wait for RandaoCommitDelay")
		return err
	}

	commitmentHash, err := task.CommitmentHash()
	if err != nil {
		return fmt.Errorf("generate commitment hash: %w", err)
	}

	// Commit PrevRandao which will be used as seed to randomly select subset of validators
	// https://github.com/Snowfork/snowbridge/blob/75a475cbf8fc8e13577ad6b773ac452b2bf82fbb/contracts/contracts/BeefyClient.sol#L446-L447
	tx, err = wr.contract.CommitPrevRandao(
		wr.conn.MakeTxOpts(ctx),
		*commitmentHash,
	)
	_, err = wr.conn.WatchTransaction(ctx, tx, 1)
	if err != nil {
		log.WithError(err).Error("Failed to CommitPrevRandao")
		return err
	}

	// Final submission
	tx, err = wr.doSubmitFinal(ctx, *commitmentHash, initialBitfield, &task)
	if err != nil {
		log.WithError(err).Error("Failed to send final signature commitment")
		return err
	}

	_, err = wr.conn.WatchTransaction(ctx, tx, 0)
	if err != nil {
		log.WithError(err).Error("Failed to submitFinal")
		return err
	}

	log.WithFields(logrus.Fields{"tx": tx.Hash().Hex(), "blockNumber": task.SignedCommitment.Commitment.BlockNumber}).Debug("Transaction SubmitFinal succeeded")

	return nil

}

func (wr *EthereumWriter) doSubmitInitial(ctx context.Context, task *Request) (*types.Transaction, []*big.Int, error) {
	signedValidators := []*big.Int{}
	for i, signature := range task.SignedCommitment.Signatures {
		if signature.IsSome() {
			signedValidators = append(signedValidators, big.NewInt(int64(i)))
		}
	}
	validatorCount := big.NewInt(int64(len(task.SignedCommitment.Signatures)))

	// Pick a random validator who signs beefy commitment
	chosenValidator := signedValidators[rand.Intn(len(signedValidators))].Int64()

	log.WithFields(logrus.Fields{
		"validatorCount":   validatorCount,
		"signedValidators": signedValidators,
		"chosenValidator":  chosenValidator,
	}).Info("Creating initial bitfield")

	initialBitfield, err := wr.contract.CreateInitialBitfield(
		&bind.CallOpts{
			Pending: true,
			From:    wr.conn.Keypair().CommonAddress(),
		},
		signedValidators, validatorCount,
	)
	if err != nil {
		return nil, nil, fmt.Errorf("create initial bitfield: %w", err)
	}

	msg, err := task.MakeSubmitInitialParams(chosenValidator, initialBitfield)
	if err != nil {
		return nil, nil, err
	}

	var tx *types.Transaction
	tx, err = wr.contract.SubmitInitial(
		wr.conn.MakeTxOpts(ctx),
		msg.Commitment,
		msg.Bitfield,
		msg.Proof,
	)
	if err != nil {
		return nil, nil, fmt.Errorf("initial submit: %w", err)
	}

	commitmentHash, err := task.CommitmentHash()
	if err != nil {
		return nil, nil, fmt.Errorf("create commitment hash: %w", err)
	}
	log.WithFields(logrus.Fields{
		"txHash":         tx.Hash().Hex(),
		"CommitmentHash": "0x" + hex.EncodeToString(commitmentHash[:]),
		"Commitment":     commitmentToLog(msg.Commitment),
		"Bitfield":       bitfieldToStrings(msg.Bitfield),
		"Proof":          proofToLog(msg.Proof),
	}).Info("Transaction submitted for initial verification")

	return tx, initialBitfield, nil
}

// doFinalSubmit sends a SubmitFinal tx to the BeefyClient contract
func (wr *EthereumWriter) doSubmitFinal(ctx context.Context, commitmentHash [32]byte, initialBitfield []*big.Int, task *Request) (*types.Transaction, error) {
	finalBitfield, err := wr.contract.CreateFinalBitfield(
		&bind.CallOpts{
			Pending: true,
			From:    wr.conn.Keypair().CommonAddress(),
		},
		commitmentHash,
		initialBitfield,
	)

	if err != nil {
		return nil, fmt.Errorf("create validator bitfield: %w", err)
	}

	validatorIndices := bitfield.New(finalBitfield).Members()

	params, err := task.MakeSubmitFinalParams(validatorIndices, initialBitfield)
	if err != nil {
		return nil, err
	}

	logFields, err := wr.makeSubmitFinalLogFields(task, params)
	if err != nil {
		return nil, fmt.Errorf("logging params: %w", err)
	}

	tx, err := wr.contract.SubmitFinal(
		wr.conn.MakeTxOpts(ctx),
		params.Commitment,
		params.Bitfield,
		params.Proofs,
		params.Leaf,
		params.LeafProof,
		params.LeafProofOrder,
	)
	if err != nil {
		return nil, fmt.Errorf("final submission: %w", err)
	}

	log.WithField("txHash", tx.Hash().Hex()).
		WithFields(logFields).
		Info("Sent SubmitFinal transaction")

	return tx, nil
}
