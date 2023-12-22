package syncer

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestCalculateNextCheckpointSlot(t *testing.T) {
	values := []struct {
		name     string
		slot     uint64
		expected uint64
	}{
		{
			name:     "slot 41",
			slot:     41,
			expected: 64,
		},
		{
			name:     "slot 64",
			slot:     64,
			expected: 64,
		},
		{
			name:     "slot 78",
			slot:     78,
			expected: 128,
		},
	}

	syncer := Syncer{}
	syncer.setting.SlotsInEpoch = 8
	syncer.setting.EpochsPerSyncCommitteePeriod = 8

	for _, tt := range values {
		result := syncer.CalculateNextCheckpointSlot(tt.slot)
		assert.Equal(t, tt.expected, result, "expected %t but found %t for slot %d", tt.expected, result, tt.slot)
	}
}
