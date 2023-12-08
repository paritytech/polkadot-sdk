package parachain

import (
	"context"
	"encoding/hex"
	"log"
	"os"
	"os/signal"
	"reflect"
	"strings"
	"syscall"

	"github.com/mitchellh/mapstructure"
	"github.com/sirupsen/logrus"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/snowfork/snowbridge/relayer/relays/parachain"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
	"golang.org/x/sync/errgroup"
)

var (
	configFile     string
	privateKey     string
	privateKeyFile string
)

func Command() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "parachain",
		Short: "Start the parachain relay",
		Args:  cobra.ExactArgs(0),
		RunE:  run,
	}

	cmd.Flags().StringVar(&configFile, "config", "", "Path to configuration file")
	cmd.MarkFlagRequired("config")

	cmd.Flags().StringVar(&privateKey, "ethereum.private-key", "", "Ethereum private key")
	cmd.Flags().StringVar(&privateKeyFile, "ethereum.private-key-file", "", "The file from which to read the private key")

	return cmd
}

func run(_ *cobra.Command, _ []string) error {
	log.SetOutput(logrus.WithFields(logrus.Fields{"logger": "stdlib"}).WriterLevel(logrus.InfoLevel))
	logrus.SetLevel(logrus.DebugLevel)

	viper.SetConfigFile(configFile)
	if err := viper.ReadInConfig(); err != nil {
		return err
	}

	var config parachain.Config
	err := viper.Unmarshal(&config, viper.DecodeHook(HexHookFunc()))
	if err != nil {
		return err
	}

	keypair, err := ethereum.ResolvePrivateKey(privateKey, privateKeyFile)
	if err != nil {
		return err
	}

	relay, err := parachain.NewRelay(&config, keypair)
	if err != nil {
		return err
	}

	ctx, cancel := context.WithCancel(context.Background())
	eg, ctx := errgroup.WithContext(ctx)

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

	err = relay.Start(ctx, eg)
	if err != nil {
		logrus.WithError(err).Fatal("Unhandled error")
		cancel()
		return err
	}

	err = eg.Wait()
	if err != nil {
		logrus.WithError(err).Fatal("Unhandled error")
		return err
	}

	return nil
}

func HexHookFunc() mapstructure.DecodeHookFuncType {
	return func(
		f reflect.Type,
		t reflect.Type,
		data interface{},
	) (interface{}, error) {
		// Check that the data is string
		if f.Kind() != reflect.String {
			return data, nil
		}

		// Check that the target type is our custom type
		if t != reflect.TypeOf(parachain.ChannelID{}) {
			return data, nil
		}

		foo, err := HexDecodeString(data.(string))
		if err != nil {
			return nil, err
		}

		var out [32]byte
		copy(out[:], foo)

		// Return the parsed value
		return parachain.ChannelID(out), nil
	}
}

// HexDecodeString decodes bytes from a hex string. Contrary to hex.DecodeString, this function does not error if "0x"
// is prefixed, and adds an extra 0 if the hex string has an odd length.
func HexDecodeString(s string) ([]byte, error) {
	s = strings.TrimPrefix(s, "0x")

	if len(s)%2 != 0 {
		s = "0" + s
	}

	b, err := hex.DecodeString(s)
	if err != nil {
		return nil, err
	}

	return b, nil
}
