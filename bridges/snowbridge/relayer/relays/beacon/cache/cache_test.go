package cache

import (
	"github.com/ethereum/go-ethereum/common"
	"github.com/stretchr/testify/require"
	"testing"
)

func TestCalculateClosestCheckpointSlot(t *testing.T) {
	b := New(8, 8)

	b.AddCheckPoint(common.HexToHash("0xfa767e1fb1280799fd406bd7905d3bef62d498211183548f9ebb7a1d16edce4c"), nil, 16)
	b.AddCheckPoint(common.HexToHash("0xe5509a901249bcb4800b644ebb3c666074848ea02d0e85427fff29fe2ec354ec"), nil, 64)
	b.AddCheckPoint(common.HexToHash("0xecdf3404d4909e5ef6315566ae0cca2c20bf2e6ec6c18f4d26fc7913d9eaa592"), nil, 128)

	slot, err := b.calculateClosestCheckpointSlot(17)
	require.NoError(t, err)
	require.Equal(t, uint64(64), slot)
}

func TestCalculateClosestCheckpointSlot_WithoutCheckpointIncludingSlot(t *testing.T) {
	b := New(8, 8)

	b.AddCheckPoint(common.HexToHash("0xe5509a901249bcb4800b644ebb3c666074848ea02d0e85427fff29fe2ec354ec"), nil, 72)
	b.AddCheckPoint(common.HexToHash("0xecdf3404d4909e5ef6315566ae0cca2c20bf2e6ec6c18f4d26fc7913d9eaa592"), nil, 144)

	_, err := b.calculateClosestCheckpointSlot(2)
	require.Error(t, err)
}

func TestCalculateClosestCheckpointSlot_WithoutCheckpointIncludingSlotTooLarge(t *testing.T) {
	b := New(8, 8)

	b.AddCheckPoint(common.HexToHash("0xe5509a901249bcb4800b644ebb3c666074848ea02d0e85427fff29fe2ec354ec"), nil, 72)
	b.AddCheckPoint(common.HexToHash("0xecdf3404d4909e5ef6315566ae0cca2c20bf2e6ec6c18f4d26fc7913d9eaa592"), nil, 144)

	_, err := b.calculateClosestCheckpointSlot(145)
	require.Error(t, err)
}

func TestCalculateClosestCheckpointSlot_WithCheckpointMatchingSlot(t *testing.T) {
	b := New(8, 8)

	b.AddCheckPoint(common.HexToHash("0xe5509a901249bcb4800b644ebb3c666074848ea02d0e85427fff29fe2ec354ec"), nil, 72)
	b.AddCheckPoint(common.HexToHash("0xecdf3404d4909e5ef6315566ae0cca2c20bf2e6ec6c18f4d26fc7913d9eaa592"), nil, 144)

	slot, err := b.calculateClosestCheckpointSlot(144)
	require.NoError(t, err)
	require.Equal(t, uint64(144), slot)
}

func TestCalculateClosestCheckpointSlot_WithMoreThanOneCheckpoint(t *testing.T) {
	b := New(8, 8)

	b.AddCheckPoint(common.HexToHash("0xe5509a901249bcb4800b644ebb3c666074848ea02d0e85427fff29fe2ec354ec"), nil, 32)
	b.AddCheckPoint(common.HexToHash("0xecdf3404d4909e5ef6315566ae0cca2c20bf2e6ec6c18f4d26fc7913d9eaa592"), nil, 16)

	slot, err := b.calculateClosestCheckpointSlot(2)
	require.NoError(t, err)
	require.Equal(t, uint64(16), slot) // taking the first matching checkpoint is fine
}

func TestAddSlot(t *testing.T) {
	b := BeaconCache{}

	b.addSlot(5)
	require.Equal(t, []uint64{5}, b.Finalized.Checkpoints.Slots)

	b.addSlot(10)
	require.Equal(t, []uint64{5, 10}, b.Finalized.Checkpoints.Slots)

	b.addSlot(10) // test duplicate slot add
	require.Equal(t, []uint64{5, 10}, b.Finalized.Checkpoints.Slots)

	b.addSlot(6) // test duplicate slot add
	require.Equal(t, []uint64{5, 6, 10}, b.Finalized.Checkpoints.Slots)
}

func TestAddPruneOldCheckpoints(t *testing.T) {
	b := New(8, 8)

	for i := 1; i <= FinalizedCheckpointsLimit+20; i++ {
		b.AddCheckPoint(common.HexToHash("0xe5509a901249bcb4800b644ebb3c666074848ea02d0e85427fff29fe2ec354ec"), nil, uint64(i))
	}

	require.Equal(t, FinalizedCheckpointsLimit, len(b.Finalized.Checkpoints.Slots))

	for _, checkpoint := range b.Finalized.Checkpoints.Proofs {
		// check that each slot is within the expected range
		require.Greater(t, checkpoint.Slot, uint64(20))
		require.LessOrEqual(t, checkpoint.Slot, uint64(FinalizedCheckpointsLimit+20))
	}
}
