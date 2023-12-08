// Copyright 2021 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package relays

type WorkerConfig struct {
	// Should this worker run?
	Enabled bool `mapstructure:"enabled"`
	// Restart delay in seconds
	RestartDelay uint `mapstructure:"restart-delay"`
}
