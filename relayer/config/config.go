package config

type PolkadotConfig struct {
	Endpoint string `mapstructure:"endpoint"`
}

type ParachainConfig struct {
	Endpoint             string `mapstructure:"endpoint"`
	MaxWatchedExtrinsics int64  `mapstructure:"maxWatchedExtrinsics"`
	MaxBatchCallSize     int64  `mapstructure:"maxBatchCallSize"`
}

type EthereumConfig struct {
	Endpoint  string `mapstructure:"endpoint"`
	GasFeeCap uint64 `mapstructure:"gas-fee-cap"`
	GasTipCap uint64 `mapstructure:"gas-tip-cap"`
	GasLimit  uint64 `mapstructure:"gas-limit"`
}
