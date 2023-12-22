package cmd

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/sirupsen/logrus"
	"github.com/snowfork/snowbridge/relayer/chain/relaychain"
	"github.com/snowfork/snowbridge/relayer/relays/beefy"
	"github.com/spf13/cobra"
	"golang.org/x/sync/errgroup"
)

func scanBeefyCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "scan-beefy",
		Short: "Scan beefy messages like the beefy relayer would.",
		Args:  cobra.ExactArgs(0),
		RunE:  ScanBeefyFn,
	}

	cmd.Flags().StringP("polkadot-url", "p", "ws://127.0.0.1:9944", "Polkadot URL.")
	cmd.MarkFlagRequired("polkadot-url")
	cmd.Flags().Uint64P("beefy-block", "b", 0, "Beefy block.")
	cmd.MarkFlagRequired("beefy-block")
	cmd.Flags().Uint64P("validator-set-id", "v", 0, "Validator set id.")
	cmd.MarkFlagRequired("validator-set-id")
	cmd.Flags().Uint64P("fast-forward-depth", "f", 100, "Fast forward depth.")
	return cmd
}

func ScanBeefyFn(cmd *cobra.Command, _ []string) error {
	ctx := cmd.Context()
	ctx, cancel := context.WithCancel(context.Background())
	eg, ctx := errgroup.WithContext(ctx)

	log.SetOutput(logrus.WithFields(logrus.Fields{"logger": "stdlib"}).WriterLevel(logrus.InfoLevel))
	logrus.SetLevel(logrus.DebugLevel)

	polkadotUrl, _ := cmd.Flags().GetString("polkadot-url")
	relaychainConn := relaychain.NewConnection(polkadotUrl)
	relaychainConn.Connect(ctx)

	fastForwardDepth, _ := cmd.Flags().GetUint64("fast-forward-depth")
	config := beefy.SourceConfig{
		FastForwardDepth: fastForwardDepth,
	}
	polkadotListener := beefy.NewPolkadotListener(
		&config,
		relaychainConn,
	)

	beefyBlock, _ := cmd.Flags().GetUint64("beefy-block")
	validatorSetID, _ := cmd.Flags().GetUint64("validator-set-id")
	logrus.WithFields(logrus.Fields{
		"polkadot-url":       polkadotUrl,
		"fast-forward-depth": fastForwardDepth,
		"beefy-block":        beefyBlock,
		"validator-set-id":   validatorSetID,
	}).Info("Connected to relaychain.")

	commitments, err := polkadotListener.Start(ctx, eg, beefyBlock, validatorSetID)
	if err != nil {
		logrus.WithError(err).Fatalf("could not start")
	}

	eg.Go(func() error {
		for {
			select {
			case <-ctx.Done():
				return nil
			case commitment, ok := <-commitments:
				if !ok {
					return nil
				}
				logrus.WithField("commitment", commitment).Info("scanned commitment")
			}
		}
	})

	// Ensure clean termination upon SIGINT, SIGTERM
	eg.Go(func() error {
		notify := make(chan os.Signal, 1)
		signal.Notify(notify, syscall.SIGINT, syscall.SIGTERM)

		select {
		case <-ctx.Done():
			return ctx.Err()
		case sig := <-notify:
			logrus.WithField("signal", sig.String()).Info("Received signal")
			cancel()
		}

		return nil
	})

	err = eg.Wait()
	if err != nil {
		logrus.WithError(err).Fatal("Unhandled error")
		return err
	}

	return nil
}
