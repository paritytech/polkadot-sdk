package json

import (
	"strings"
)

type CheckPoint struct {
	Header                     BeaconHeader  `json:"header"`
	CurrentSyncCommittee       SyncCommittee `json:"current_sync_committee"`
	CurrentSyncCommitteeBranch []string      `json:"current_sync_committee_branch"`
	ValidatorsRoot             string        `json:"validators_root"`
	BlockRootsRoot             string        `json:"block_roots_root"`
	BlockRootsBranch           []string      `json:"block_roots_branch"`
}

type BeaconHeader struct {
	Slot          uint64 `json:"slot"`
	ProposerIndex uint64 `json:"proposer_index"`
	ParentRoot    string `json:"parent_root"`
	StateRoot     string `json:"state_root"`
	BodyRoot      string `json:"body_root"`
}

type SyncCommittee struct {
	Pubkeys         []string `json:"pubkeys"`
	AggregatePubkey string   `json:"aggregate_pubkey"`
}

type SyncAggregate struct {
	SyncCommitteeBits      string `json:"sync_committee_bits"`
	SyncCommitteeSignature string `json:"sync_committee_signature"`
}

type Update struct {
	AttestedHeader          BeaconHeader             `json:"attested_header"`
	SyncAggregate           SyncAggregate            `json:"sync_aggregate"`
	SignatureSlot           uint64                   `json:"signature_slot"`
	NextSyncCommitteeUpdate *NextSyncCommitteeUpdate `json:"next_sync_committee_update"`
	FinalizedHeader         BeaconHeader             `json:"finalized_header"`
	FinalityBranch          []string                 `json:"finality_branch"`
	BlockRootsRoot          string                   `json:"block_roots_root"`
	BlockRootsBranch        []string                 `json:"block_roots_branch"`
}

type NextSyncCommitteeUpdate struct {
	NextSyncCommittee       SyncCommittee `json:"next_sync_committee"`
	NextSyncCommitteeBranch []string      `json:"next_sync_committee_branch"`
}

type ProposerSlashing struct {
	SignedHeader1 SignedHeader `json:"signed_header_1"`
	SignedHeader2 SignedHeader `json:"signed_header_2"`
}

type AttesterSlashing struct {
	Attestation1 IndexedAttestation `json:"attestation_1"`
	Attestation2 IndexedAttestation `json:"attestation_2"`
}

type IndexedAttestation struct {
	AttestingIndices []uint64        `json:"attesting_indices"`
	Data             AttestationData `json:"data"`
	Signature        string          `json:"signature"`
}

type AttestationData struct {
	Slot            uint64     `json:"slot"`
	Index           uint64     `json:"index"`
	BeaconBlockRoot string     `json:"beacon_block_root"`
	Source          Checkpoint `json:"source"`
	Target          Checkpoint `json:"target"`
}

type Checkpoint struct {
	Epoch uint64 `json:"epoch"`
	Root  string `json:"root"`
}

type SignedHeader struct {
	Message   BeaconHeader `json:"message"`
	Signature string       `json:"signature"`
}

type Block struct {
	Slot          uint64    `json:"slot"`
	ProposerIndex uint64    `json:"proposer_index"`
	ParentRoot    string    `json:"parent_root"`
	StateRoot     string    `json:"state_root"`
	Body          BlockBody `json:"body"`
}

type ExecutionPayloadHeaderCapella struct {
	ParentHash      string `json:"parent_hash"`
	FeeRecipient    string `json:"fee_recipient"`
	StateRoot       string `json:"state_root"`
	ReceiptsRoot    string `json:"receipts_root"`
	LogsBloom       string `json:"logs_bloom"`
	PrevRandao      string `json:"prev_randao"`
	BlockNumber     uint64 `json:"block_number"`
	GasLimit        uint64 `json:"gas_limit"`
	GasUsed         uint64 `json:"gas_used"`
	Timestamp       uint64 `json:"timestamp"`
	ExtraData       string `json:"extra_data"`
	BaseFeePerGas   uint64 `json:"base_fee_per_gas"`
	BlockHash       string `json:"block_hash"`
	TransactionRoot string `json:"transactions_root"`
	WithdrawalsRoot string `json:"withdrawals_root"`
}

type Eth1Data struct {
	DepositRoot  string `json:"deposit_root"`
	DepositCount uint64 `json:"deposit_count"`
	BlockHash    string `json:"block_hash"`
}

type BlockBody struct {
	RandaoReveal      string                        `json:"randao_reveal"`
	Eth1Data          Eth1Data                      `json:"eth1_data"`
	Graffiti          string                        `json:"graffiti"`
	ProposerSlashings []ProposerSlashing            `json:"proposer_slashings"`
	AttesterSlashings []AttesterSlashing            `json:"attester_slashings"`
	Attestations      []Attestation                 `json:"attestations"`
	Deposits          []Deposit                     `json:"deposits"`
	VoluntaryExits    []VoluntaryExit               `json:"voluntary_exits"`
	SyncAggregate     SyncAggregate                 `json:"sync_aggregate"`
	ExecutionPayload  ExecutionPayloadHeaderCapella `json:"execution_payload"`
}

type HeaderUpdate struct {
	Header          BeaconHeader                  `json:"header"`
	AncestryProof   *AncestryProof                `json:"ancestry_proof"`
	ExecutionHeader ExecutionPayloadHeaderCapella `json:"execution_header"`
	ExecutionBranch []string                      `json:"execution_branch"`
}

type AncestryProof struct {
	HeaderBranch       []string `json:"header_branch"`
	FinalizedBlockRoot string   `json:"finalized_block_root"`
}

type Attestation struct {
	AggregationBits string          `json:"aggregation_bits"`
	Data            AttestationData `json:"data"`
	Signature       string          `json:"signature"`
}

type DepositData struct {
	Pubkey                string `json:"pubkey"`
	WithdrawalCredentials string `json:"withdrawal_credentials"`
	Amount                uint64 `json:"amount"`
	Signature             string `json:"signature"`
}

type VoluntaryExit struct {
	Epoch          uint64 `json:"epoch"`
	ValidatorIndex uint64 `json:"validator_index"`
}

type Deposit struct {
	Proof []string    `json:"proof"`
	Data  DepositData `json:"data"`
}

func (b *BeaconHeader) RemoveLeadingZeroHashes() {
	b.ParentRoot = removeLeadingZeroHash(b.ParentRoot)
	b.StateRoot = removeLeadingZeroHash(b.StateRoot)
	b.BodyRoot = removeLeadingZeroHash(b.BodyRoot)
}

func (s *SyncCommittee) RemoveLeadingZeroHashes() {
	for i, pubkey := range s.Pubkeys {
		s.Pubkeys[i] = removeLeadingZeroHash(pubkey)
	}

	s.AggregatePubkey = removeLeadingZeroHash(s.AggregatePubkey)
}

func (p *ProposerSlashing) RemoveLeadingZeroHashes() {
	p.SignedHeader1.RemoveLeadingZeroHashes()
	p.SignedHeader2.RemoveLeadingZeroHashes()
}

func (a *AttesterSlashing) RemoveLeadingZeroHashes() {
	a.Attestation1.RemoveLeadingZeroHashes()
	a.Attestation2.RemoveLeadingZeroHashes()
}

func (i *IndexedAttestation) RemoveLeadingZeroHashes() {
	i.Data.RemoveLeadingZeroHashes()
	i.Signature = removeLeadingZeroHash(i.Signature)
}

func (a *AttestationData) RemoveLeadingZeroHashes() {
	a.BeaconBlockRoot = removeLeadingZeroHash(a.BeaconBlockRoot)
	a.Source.RemoveLeadingZeroHashes()
	a.Target.RemoveLeadingZeroHashes()
}

func (s *SignedHeader) RemoveLeadingZeroHashes() {
	s.Message.RemoveLeadingZeroHashes()
	s.Signature = removeLeadingZeroHash(s.Signature)
}

func (s *SyncAggregate) RemoveLeadingZeroHashes() {
	s.SyncCommitteeBits = removeLeadingZeroHash(s.SyncCommitteeBits)
	s.SyncCommitteeSignature = removeLeadingZeroHash(s.SyncCommitteeSignature)
}

func (a *Attestation) RemoveLeadingZeroHashes() {
	a.AggregationBits = removeLeadingZeroHash(a.AggregationBits)
	a.Data.RemoveLeadingZeroHashes()
	a.Signature = removeLeadingZeroHash(a.Signature)
}

func (c *Checkpoint) RemoveLeadingZeroHashes() {
	c.Root = removeLeadingZeroHash(c.Root)
}

func (d *Deposit) RemoveLeadingZeroHashes() {
	d.Data.Pubkey = removeLeadingZeroHash(d.Data.Pubkey)
	d.Data.Signature = removeLeadingZeroHash(d.Data.Signature)
	d.Data.WithdrawalCredentials = removeLeadingZeroHash(d.Data.WithdrawalCredentials)
}

func (b *Block) RemoveLeadingZeroHashes() {
	b.ParentRoot = removeLeadingZeroHash(b.ParentRoot)
	b.StateRoot = removeLeadingZeroHash(b.StateRoot)
	b.Body.RandaoReveal = removeLeadingZeroHash(b.Body.RandaoReveal)
	b.Body.Eth1Data.DepositRoot = removeLeadingZeroHash(b.Body.Eth1Data.DepositRoot)
	b.Body.Eth1Data.BlockHash = removeLeadingZeroHash(b.Body.Eth1Data.BlockHash)
	b.Body.Graffiti = removeLeadingZeroHash(b.Body.Graffiti)

	for i := range b.Body.ProposerSlashings {
		b.Body.ProposerSlashings[i].RemoveLeadingZeroHashes()
	}

	for i := range b.Body.AttesterSlashings {
		b.Body.AttesterSlashings[i].RemoveLeadingZeroHashes()
	}

	for i := range b.Body.Attestations {
		b.Body.Attestations[i].RemoveLeadingZeroHashes()
	}

	for i := range b.Body.Deposits {
		b.Body.Deposits[i].RemoveLeadingZeroHashes()
	}

	b.Body.SyncAggregate.RemoveLeadingZeroHashes()
	b.Body.ExecutionPayload.RemoveLeadingZeroHashes()
}

func (e *ExecutionPayloadHeaderCapella) RemoveLeadingZeroHashes() {
	e.ParentHash = removeLeadingZeroHash(e.ParentHash)
	e.FeeRecipient = removeLeadingZeroHash(e.FeeRecipient)
	e.StateRoot = removeLeadingZeroHash(e.StateRoot)
	e.ReceiptsRoot = removeLeadingZeroHash(e.ReceiptsRoot)
	e.LogsBloom = removeLeadingZeroHash(e.LogsBloom)
	e.PrevRandao = removeLeadingZeroHash(e.PrevRandao)
	e.ExtraData = removeLeadingZeroHash(e.ExtraData)
	e.BlockHash = removeLeadingZeroHash(e.BlockHash)
	e.TransactionRoot = removeLeadingZeroHash(e.TransactionRoot)
	e.WithdrawalsRoot = removeLeadingZeroHash(e.WithdrawalsRoot)
}

func (i *CheckPoint) RemoveLeadingZeroHashes() {
	i.Header.RemoveLeadingZeroHashes()
	i.CurrentSyncCommittee.RemoveLeadingZeroHashes()

	for k, branch := range i.CurrentSyncCommitteeBranch {
		i.CurrentSyncCommitteeBranch[k] = removeLeadingZeroHash(branch)
	}

	i.ValidatorsRoot = removeLeadingZeroHash(i.ValidatorsRoot)
	i.BlockRootsRoot = removeLeadingZeroHash(i.BlockRootsRoot)
	i.BlockRootsBranch = removeLeadingZeroHashForSlice(i.BlockRootsBranch)
}

func (s *Update) RemoveLeadingZeroHashes() {
	s.AttestedHeader.RemoveLeadingZeroHashes()
	s.SyncAggregate.RemoveLeadingZeroHashes()
	if s.NextSyncCommitteeUpdate != nil {
		s.NextSyncCommitteeUpdate.NextSyncCommittee.RemoveLeadingZeroHashes()
		s.NextSyncCommitteeUpdate.NextSyncCommitteeBranch = removeLeadingZeroHashForSlice(s.NextSyncCommitteeUpdate.NextSyncCommitteeBranch)
	}
	s.FinalizedHeader.RemoveLeadingZeroHashes()
	s.FinalityBranch = removeLeadingZeroHashForSlice(s.FinalityBranch)
	s.BlockRootsRoot = removeLeadingZeroHash(s.BlockRootsRoot)
	s.BlockRootsBranch = removeLeadingZeroHashForSlice(s.BlockRootsBranch)

}

func (h *HeaderUpdate) RemoveLeadingZeroHashes() {
	h.Header.RemoveLeadingZeroHashes()
	if h.AncestryProof != nil {
		h.AncestryProof.HeaderBranch = removeLeadingZeroHashForSlice(h.AncestryProof.HeaderBranch)
		h.AncestryProof.FinalizedBlockRoot = removeLeadingZeroHash(h.AncestryProof.FinalizedBlockRoot)
	}
	h.ExecutionHeader.RemoveLeadingZeroHashes()
	h.ExecutionBranch = removeLeadingZeroHashForSlice(h.ExecutionBranch)
}

func removeLeadingZeroHashForSlice(s []string) []string {
	result := make([]string, len(s))

	for i, item := range s {
		result[i] = removeLeadingZeroHash(item)
	}
	return result
}

func removeLeadingZeroHash(s string) string {
	return strings.Replace(s, "0x", "", 1)
}
