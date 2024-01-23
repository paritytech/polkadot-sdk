package beefy

import (
	"github.com/snowfork/snowbridge/relayer/config"
)

type Config struct {
	Source SourceConfig `mapstructure:"source"`
	Sink   SinkConfig   `mapstructure:"sink"`
}

type SourceConfig struct {
	Polkadot config.PolkadotConfig `mapstructure:"polkadot"`
	// Depth to ignore the beefy updates too far away (in number of blocks)
	FastForwardDepth uint64 `mapstructure:"fast-forward-depth"`
	// Period to sample the beefy updates (in number of blocks)
	UpdatePeriod uint64 `mapstructure:"update-period"`
}

type SinkConfig struct {
	Ethereum              config.EthereumConfig `mapstructure:"ethereum"`
	DescendantsUntilFinal uint64                `mapstructure:"descendants-until-final"`
	Contracts             ContractsConfig       `mapstructure:"contracts"`
}

type ContractsConfig struct {
	BeefyClient string `mapstructure:"BeefyClient"`
}
