package beacon

import (
	"context"
	"github.com/snowfork/snowbridge/relayer/chain/parachain"
	"github.com/snowfork/snowbridge/relayer/crypto/sr25519"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/config"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header"
	"golang.org/x/sync/errgroup"
)

type Relay struct {
	config  *config.Config
	keypair *sr25519.Keypair
}

func NewRelay(
	config *config.Config,
	keypair *sr25519.Keypair,
) *Relay {
	return &Relay{
		config:  config,
		keypair: keypair,
	}
}

func (r *Relay) Start(ctx context.Context, eg *errgroup.Group) error {
	specSettings := r.config.GetSpecSettings()

	paraconn := parachain.NewConnection(r.config.Sink.Parachain.Endpoint, r.keypair.AsKeyringPair())

	err := paraconn.Connect(ctx)
	if err != nil {
		return err
	}

	writer := parachain.NewParachainWriter(
		paraconn,
		r.config.Sink.Parachain.MaxWatchedExtrinsics,
		r.config.Sink.Parachain.MaxBatchCallSize,
	)

	err = writer.Start(ctx, eg)
	if err != nil {
		return err
	}

	headers := header.New(
		writer,
		r.config.Source.Beacon.Endpoint,
		specSettings,
		r.config.GetActiveSpec(),
	)

	return headers.Sync(ctx, eg)
}
