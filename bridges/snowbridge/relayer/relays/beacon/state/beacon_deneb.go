package state

import (
	ssz "github.com/ferranbt/fastssz"
)

type ExecutionPayloadDeneb struct {
	ParentHash    [32]byte      `ssz-size:"32" json:"parent_hash"`
	FeeRecipient  [20]byte      `ssz-size:"20" json:"fee_recipient"`
	StateRoot     [32]byte      `ssz-size:"32" json:"state_root"`
	ReceiptsRoot  [32]byte      `ssz-size:"32" json:"receipts_root"`
	LogsBloom     [256]byte     `ssz-size:"256" json:"logs_bloom"`
	PrevRandao    [32]byte      `ssz-size:"32" json:"prev_randao"`
	BlockNumber   uint64        `json:"block_number"`
	GasLimit      uint64        `json:"gas_limit"`
	GasUsed       uint64        `json:"gas_used"`
	Timestamp     uint64        `json:"timestamp"`
	ExtraData     []byte        `ssz-max:"32" json:"extra_data"`
	BaseFeePerGas [32]byte      `ssz-size:"32" json:"base_fee_per_gas"`
	BlockHash     [32]byte      `ssz-size:"32" json:"block_hash"`
	Transactions  [][]byte      `ssz-max:"1048576,1073741824" ssz-size:"?,?" json:"transactions"`
	Withdrawals   []*Withdrawal `ssz-max:"16" json:"withdrawals"`
	BlobGasUsed   uint64        `json:"blob_gas_used,omitempty"`
	ExcessBlobGas uint64        `json:"excess_blob_gas,omitempty"`
}

type ExecutionPayloadHeaderDeneb struct {
	ParentHash       []byte `json:"parent_hash" ssz-size:"32"`
	FeeRecipient     []byte `json:"fee_recipient" ssz-size:"20"`
	StateRoot        []byte `json:"state_root" ssz-size:"32"`
	ReceiptsRoot     []byte `json:"receipts_root" ssz-size:"32"`
	LogsBloom        []byte `json:"logs_bloom" ssz-size:"256"`
	PrevRandao       []byte `json:"prev_randao" ssz-size:"32"`
	BlockNumber      uint64 `json:"block_number"`
	GasLimit         uint64 `json:"gas_limit"`
	GasUsed          uint64 `json:"gas_used"`
	Timestamp        uint64 `json:"timestamp"`
	ExtraData        []byte `json:"extra_data" ssz-max:"32"`
	BaseFeePerGas    []byte `json:"base_fee_per_gas" ssz-size:"32"`
	BlockHash        []byte `json:"block_hash" ssz-size:"32"`
	TransactionsRoot []byte `json:"transactions_root" ssz-size:"32"`
	WithdrawalsRoot  []byte `json:"withdrawals_root" ssz-size:"32"`
	BlobGasUsed      uint64 `json:"blob_gas_used,omitempty"`
	ExcessBlobGas    uint64 `json:"excess_blob_gas,omitempty"`
}

type BeaconBlockDenebMinimal struct {
	Slot          uint64                       `json:"slot"`
	ProposerIndex uint64                       `json:"proposer_index"`
	ParentRoot    []byte                       `json:"parent_root" ssz-size:"32"`
	StateRoot     []byte                       `json:"state_root" ssz-size:"32"`
	Body          *BeaconBlockBodyDenebMinimal `json:"body"`
}

type BeaconBlockDenebMainnet struct {
	Slot          uint64                       `json:"slot"`
	ProposerIndex uint64                       `json:"proposer_index"`
	ParentRoot    []byte                       `json:"parent_root" ssz-size:"32"`
	StateRoot     []byte                       `json:"state_root" ssz-size:"32"`
	Body          *BeaconBlockBodyDenebMainnet `json:"body"`
}

type BeaconBlockBodyDenebMinimal struct {
	RandaoReveal          []byte                        `json:"randao_reveal" ssz-size:"96"`
	Eth1Data              *Eth1Data                     `json:"eth1_data"`
	Graffiti              [32]byte                      `json:"graffiti" ssz-size:"32"`
	ProposerSlashings     []*ProposerSlashing           `json:"proposer_slashings" ssz-max:"16"`
	AttesterSlashings     []*AttesterSlashing           `json:"attester_slashings" ssz-max:"2"`
	Attestations          []*Attestation                `json:"attestations" ssz-max:"128"`
	Deposits              []*Deposit                    `json:"deposits" ssz-max:"16"`
	VoluntaryExits        []*SignedVoluntaryExit        `json:"voluntary_exits" ssz-max:"16"`
	SyncAggregate         *SyncAggregateMinimal         `json:"sync_aggregate"`
	ExecutionPayload      *ExecutionPayloadDeneb        `json:"execution_payload"`
	BlsToExecutionChanges []*SignedBLSToExecutionChange `json:"bls_to_execution_changes,omitempty" ssz-max:"16"`
	BlobKzgCommitments    [][48]byte                    `json:"blob_kzg_commitments,omitempty" ssz-max:"16" ssz-size:"?,48"`
}

type BeaconBlockBodyDenebMainnet struct {
	RandaoReveal          []byte                        `json:"randao_reveal" ssz-size:"96"`
	Eth1Data              *Eth1Data                     `json:"eth1_data"`
	Graffiti              [32]byte                      `json:"graffiti" ssz-size:"32"`
	ProposerSlashings     []*ProposerSlashing           `json:"proposer_slashings" ssz-max:"16"`
	AttesterSlashings     []*AttesterSlashing           `json:"attester_slashings" ssz-max:"2"`
	Attestations          []*Attestation                `json:"attestations" ssz-max:"128"`
	Deposits              []*Deposit                    `json:"deposits" ssz-max:"16"`
	VoluntaryExits        []*SignedVoluntaryExit        `json:"voluntary_exits" ssz-max:"16"`
	SyncAggregate         *SyncAggregateMainnet         `json:"sync_aggregate"`
	ExecutionPayload      *ExecutionPayloadDeneb        `json:"execution_payload"`
	BlsToExecutionChanges []*SignedBLSToExecutionChange `json:"bls_to_execution_changes,omitempty" ssz-max:"16"`
	BlobKzgCommitments    [][48]byte                    `json:"blob_kzg_commitments,omitempty" ssz-max:"4096" ssz-size:"?,48"`
}

type BeaconStateDenebMainnet struct {
	GenesisTime                  uint64                       `json:"genesis_time"`
	GenesisValidatorsRoot        []byte                       `json:"genesis_validators_root" ssz-size:"32"`
	Slot                         uint64                       `json:"slot"`
	Fork                         *Fork                        `json:"fork"`
	LatestBlockHeader            *BeaconBlockHeader           `json:"latest_block_header"`
	BlockRoots                   [][]byte                     `json:"block_roots" ssz-size:"8192,32"`
	StateRoots                   [][]byte                     `json:"state_roots" ssz-size:"8192,32"`
	HistoricalRoots              [][]byte                     `json:"historical_roots" ssz-max:"16777216" ssz-size:"?,32"`
	Eth1Data                     *Eth1Data                    `json:"eth1_data"`
	Eth1DataVotes                []*Eth1Data                  `json:"eth1_data_votes" ssz-max:"2048"`
	Eth1DepositIndex             uint64                       `json:"eth1_deposit_index"`
	Validators                   []*Validator                 `json:"validators" ssz-max:"1099511627776"`
	Balances                     []uint64                     `json:"balances" ssz-max:"1099511627776"`
	RandaoMixes                  [][]byte                     `json:"randao_mixes" ssz-size:"65536,32"`
	Slashings                    []uint64                     `json:"slashings" ssz-size:"8192"`
	PreviousEpochParticipation   []byte                       `json:"previous_epoch_participation" ssz-max:"1099511627776"`
	CurrentEpochParticipation    []byte                       `json:"current_epoch_participation" ssz-max:"1099511627776"`
	JustificationBits            []byte                       `json:"justification_bits" cast-type:"github.com/prysmaticlabs/go-bitfield.Bitvector4" ssz-size:"1"`
	PreviousJustifiedCheckpoint  *Checkpoint                  `json:"previous_justified_checkpoint"`
	CurrentJustifiedCheckpoint   *Checkpoint                  `json:"current_justified_checkpoint"`
	FinalizedCheckpoint          *Checkpoint                  `json:"finalized_checkpoint"`
	InactivityScores             []uint64                     `json:"inactivity_scores" ssz-max:"1099511627776"`
	CurrentSyncCommittee         *SyncCommittee               `json:"current_sync_committee"`
	NextSyncCommittee            *SyncCommittee               `json:"next_sync_committee"`
	LatestExecutionPayloadHeader *ExecutionPayloadHeaderDeneb `json:"latest_execution_payload_header"`
	NextWithdrawalIndex          uint64                       `json:"next_withdrawal_index,omitempty"`
	NextWithdrawalValidatorIndex uint64                       `json:"next_withdrawal_validator_index,omitempty"`
	HistoricalSummaries          []*HistoricalSummary         `json:"historical_summaries,omitempty" ssz-max:"16777216"`
}

type BeaconStateDenebMinimal struct {
	GenesisTime                  uint64                       `json:"genesis_time"`
	GenesisValidatorsRoot        []byte                       `json:"genesis_validators_root" ssz-size:"32"`
	Slot                         uint64                       `json:"slot"`
	Fork                         *Fork                        `json:"fork"`
	LatestBlockHeader            *BeaconBlockHeader           `json:"latest_block_header"`
	BlockRoots                   [][]byte                     `json:"block_roots" ssz-size:"64,32"`
	StateRoots                   [][]byte                     `json:"state_roots" ssz-size:"64,32"`
	HistoricalRoots              [][]byte                     `json:"historical_roots" ssz-max:"16777216" ssz-size:"?,32"`
	Eth1Data                     *Eth1Data                    `json:"eth1_data"`
	Eth1DataVotes                []*Eth1Data                  `json:"eth1_data_votes" ssz-max:"32"`
	Eth1DepositIndex             uint64                       `json:"eth1_deposit_index"`
	Validators                   []*Validator                 `json:"validators" ssz-max:"1099511627776"`
	Balances                     []uint64                     `json:"balances" ssz-max:"1099511627776"`
	RandaoMixes                  [][]byte                     `json:"randao_mixes" ssz-size:"64,32"`
	Slashings                    []uint64                     `json:"slashings" ssz-size:"64"`
	PreviousEpochParticipation   []byte                       `json:"previous_epoch_participation" ssz-max:"1099511627776"`
	CurrentEpochParticipation    []byte                       `json:"current_epoch_participation" ssz-max:"1099511627776"`
	JustificationBits            []byte                       `json:"justification_bits" ssz-size:"1"`
	PreviousJustifiedCheckpoint  *Checkpoint                  `json:"previous_justified_checkpoint"`
	CurrentJustifiedCheckpoint   *Checkpoint                  `json:"current_justified_checkpoint"`
	FinalizedCheckpoint          *Checkpoint                  `json:"finalized_checkpoint"`
	InactivityScores             []uint64                     `json:"inactivity_scores" ssz-max:"1099511627776"`
	CurrentSyncCommittee         *SyncCommitteeMinimal        `json:"current_sync_committee"`
	NextSyncCommittee            *SyncCommitteeMinimal        `json:"next_sync_committee"`
	LatestExecutionPayloadHeader *ExecutionPayloadHeaderDeneb `json:"latest_execution_payload_header"`
	NextWithdrawalIndex          uint64                       `json:"next_withdrawal_index,omitempty"`
	NextWithdrawalValidatorIndex uint64                       `json:"next_withdrawal_validator_index,omitempty"`
	HistoricalSummaries          []*HistoricalSummary         `json:"historical_summaries,omitempty" ssz-max:"16777216"`
}

func (b *BeaconBlockDenebMinimal) GetBeaconSlot() uint64 {
	return b.Slot
}

func (b *BeaconBlockDenebMinimal) GetBlockBodyTree() (*ssz.Node, error) {
	return b.Body.GetTree()
}

func (b *BeaconBlockDenebMainnet) GetBeaconSlot() uint64 {
	return b.Slot
}

func (b *BeaconBlockDenebMainnet) GetBlockBodyTree() (*ssz.Node, error) {
	return b.Body.GetTree()
}

func (b *BeaconStateDenebMinimal) GetSlot() uint64 {
	return b.Slot
}

func (b *BeaconStateDenebMinimal) GetLatestBlockHeader() *BeaconBlockHeader {
	return b.LatestBlockHeader
}

func (b *BeaconStateDenebMinimal) GetBlockRoots() [][]byte {
	return b.BlockRoots
}

func (b *BeaconStateDenebMainnet) GetSlot() uint64 {
	return b.Slot
}

func (b *BeaconStateDenebMainnet) GetLatestBlockHeader() *BeaconBlockHeader {
	return b.LatestBlockHeader
}

func (b *BeaconStateDenebMainnet) GetBlockRoots() [][]byte {
	return b.BlockRoots
}

func (b *BeaconStateDenebMainnet) SetBlockRoots(blockRoots [][]byte) {
	b.BlockRoots = blockRoots
}

func (b *BeaconBlockDenebMainnet) ExecutionPayloadCapella() *ExecutionPayloadCapella {
	return nil
}

func (b *BeaconBlockDenebMainnet) ExecutionPayloadDeneb() *ExecutionPayloadDeneb {
	return b.Body.ExecutionPayload
}

func (b *BeaconBlockDenebMinimal) ExecutionPayloadCapella() *ExecutionPayloadCapella {
	return nil
}

func (b *BeaconBlockDenebMinimal) ExecutionPayloadDeneb() *ExecutionPayloadDeneb {
	return b.Body.ExecutionPayload
}
