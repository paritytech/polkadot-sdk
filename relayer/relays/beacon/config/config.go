package config

import (
	"github.com/snowfork/snowbridge/relayer/config"
)

type Config struct {
	Source SourceConfig `mapstructure:"source"`
	Sink   SinkConfig   `mapstructure:"sink"`
}

type SpecSettings struct {
	SlotsInEpoch                 uint64 `mapstructure:"slotsInEpoch"`
	EpochsPerSyncCommitteePeriod uint64 `mapstructure:"epochsPerSyncCommitteePeriod"`
	DenebForkEpoch               uint64 `mapstructure:"denebForkedEpoch"`
}

type SourceConfig struct {
	Beacon BeaconConfig `mapstructure:"beacon"`
}

type BeaconConfig struct {
	Endpoint string       `mapstructure:"endpoint"`
	Spec     SpecSettings `mapstructure:"spec"`
}

type SinkConfig struct {
	Parachain config.ParachainConfig `mapstructure:"parachain"`
}
