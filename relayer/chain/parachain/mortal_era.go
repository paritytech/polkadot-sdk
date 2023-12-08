// Copyright 2020 Snowfork
// SPDX-License-Identifier: LGPL-3.0-only

package parachain

import (
	"math"

	"github.com/snowfork/go-substrate-rpc-client/v4/types"
)

// Must be a power of two between 4 and 65536 (inclusive)
const MortalEraPeriod = uint64(64)

func NewMortalEra(currentBlockNumber uint64) types.ExtrinsicEra {
	// Adapted from https://substrate.dev/rustdocs/v2.0.1/src/sp_runtime/generic/era.rs.html#66
	phase := currentBlockNumber % MortalEraPeriod

	quantizeFactor := MortalEraPeriod >> 12
	if quantizeFactor < 1 {
		quantizeFactor = 1
	}
	quantizedPhase := phase / quantizeFactor * quantizeFactor

	encoded := uint16(math.Log2(float64(MortalEraPeriod))-1) | uint16((quantizedPhase/quantizeFactor)<<4)

	return types.ExtrinsicEra{
		IsMortalEra: true,
		AsMortalEra: types.MortalEra{
			First:  byte(encoded),
			Second: byte(encoded >> 8),
		},
	}
}
