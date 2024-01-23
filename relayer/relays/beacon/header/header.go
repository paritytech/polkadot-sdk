package header

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/config"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/scale"

	log "github.com/sirupsen/logrus"
	"github.com/snowfork/snowbridge/relayer/chain/parachain"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/cache"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer"
	"golang.org/x/sync/errgroup"
)

var ErrFinalizedHeaderUnchanged = errors.New("finalized header unchanged")
var ErrFinalizedHeaderNotImported = errors.New("finalized header not imported")
var ErrSyncCommitteeNotImported = errors.New("sync committee not imported")
var ErrSyncCommitteeLatency = errors.New("sync committee latency found")
var ErrExecutionHeaderNotImported = errors.New("execution header not imported")

type Header struct {
	cache  *cache.BeaconCache
	writer *parachain.ParachainWriter
	syncer *syncer.Syncer
}

func New(writer *parachain.ParachainWriter, beaconEndpoint string, setting config.SpecSettings, activeSpec config.ActiveSpec) Header {
	return Header{
		cache:  cache.New(setting.SlotsInEpoch, setting.EpochsPerSyncCommitteePeriod),
		writer: writer,
		syncer: syncer.New(beaconEndpoint, setting, activeSpec),
	}
}

func (h *Header) Sync(ctx context.Context, eg *errgroup.Group) error {
	lastFinalizedHeaderState, err := h.writer.GetLastFinalizedHeaderState()
	if err != nil {
		return fmt.Errorf("fetch parachain last finalized header state: %w", err)
	}
	latestSyncedPeriod := h.syncer.ComputeSyncPeriodAtSlot(lastFinalizedHeaderState.BeaconSlot)
	executionHeaderState, err := h.writer.GetLastExecutionHeaderState()
	if err != nil {
		return fmt.Errorf("fetch last execution hash: %w", err)
	}

	log.WithFields(log.Fields{
		"last_finalized_hash":   lastFinalizedHeaderState.BeaconBlockRoot,
		"last_finalized_slot":   lastFinalizedHeaderState.BeaconSlot,
		"last_finalized_period": latestSyncedPeriod,
		"last_execution_hash":   executionHeaderState.BeaconBlockRoot,
		"last_execution_slot":   executionHeaderState.BeaconSlot,
	}).Info("set cache: Current state")
	h.cache.SetLastSyncedFinalizedState(lastFinalizedHeaderState.BeaconBlockRoot, lastFinalizedHeaderState.BeaconSlot)
	h.cache.SetInitialCheckpointSlot(lastFinalizedHeaderState.InitialCheckpointSlot)
	h.cache.AddCheckPointSlots([]uint64{lastFinalizedHeaderState.BeaconSlot})
	h.cache.SetLastSyncedExecutionSlot(executionHeaderState.BeaconSlot)

	log.Info("starting to sync finalized headers")

	ticker := time.NewTicker(time.Second * 10)

	eg.Go(func() error {
		for {
			err = h.SyncHeaders(ctx)
			logFields := log.Fields{
				"finalized_header": h.cache.Finalized.LastSyncedHash,
				"finalized_slot":   h.cache.Finalized.LastSyncedSlot,
			}
			switch {
			case errors.Is(err, ErrFinalizedHeaderUnchanged):
				log.WithFields(logFields).Info("not importing unchanged header")
			case errors.Is(err, ErrFinalizedHeaderNotImported):
				log.WithFields(logFields).WithError(err).Warn("Not importing header this cycle")
			case errors.Is(err, ErrSyncCommitteeNotImported):
				log.WithFields(logFields).WithError(err).Warn("SyncCommittee not imported")
			case errors.Is(err, ErrSyncCommitteeLatency):
				log.WithFields(logFields).WithError(err).Warn("SyncCommittee latency found")
			case errors.Is(err, ErrExecutionHeaderNotImported):
				log.WithFields(logFields).WithError(err).Warn("ExecutionHeader not imported")
			case errors.Is(err, syncer.ErrBeaconStateAvailableYet):
				log.WithFields(logFields).WithError(err).Warn("beacon state not available for finalized state yet")
			case err != nil:
				return err
			}

			select {
			case <-ctx.Done():
				return nil
			case <-ticker.C:
				continue
			}
		}
	})

	return nil
}

func (h *Header) SyncCommitteePeriodUpdate(ctx context.Context, period uint64) error {
	update, err := h.syncer.GetSyncCommitteePeriodUpdate(period)

	switch {
	case errors.Is(err, syncer.ErrCommitteeUpdateHeaderInDifferentSyncPeriod):
		{
			log.WithField("period", period).Info("committee update and header in different sync periods, skipping")

			return err
		}
	case err != nil:
		{
			return fmt.Errorf("fetch sync committee period update: %w", err)
		}
	}

	log.WithFields(log.Fields{
		"finalized_header_slot": update.Payload.FinalizedHeader.Slot,
		"period":                period,
	}).Info("syncing sync committee for period")

	err = h.writer.WriteToParachainAndWatch(ctx, "EthereumBeaconClient.submit", update.Payload)
	if err != nil {
		return err
	}

	// Only update cache when SyncCommitteeUpdate import succeeded and period updated as expected
	lastFinalizedHeaderState, err := h.writer.GetLastFinalizedHeaderState()
	if err != nil {
		return fmt.Errorf("fetch last finalized header state: %w", err)
	}
	lastUpdatedPeriod := h.syncer.ComputeSyncPeriodAtSlot(lastFinalizedHeaderState.BeaconSlot)
	if period != lastUpdatedPeriod {
		return ErrSyncCommitteeNotImported
	}
	h.cache.SetLastSyncedFinalizedState(update.FinalizedHeaderBlockRoot, uint64(update.Payload.FinalizedHeader.Slot))
	h.cache.AddCheckPoint(update.FinalizedHeaderBlockRoot, update.BlockRootsTree, uint64(update.Payload.FinalizedHeader.Slot))

	return nil
}

func (h *Header) SyncFinalizedHeader(ctx context.Context) error {
	// When the chain has been processed up until now, keep getting finalized block updates and send that to the parachain
	update, err := h.syncer.GetFinalizedUpdate()
	if err != nil {
		return fmt.Errorf("fetch finalized header update from Ethereum beacon client: %w", err)
	}

	log.WithFields(log.Fields{
		"slot":      update.Payload.FinalizedHeader.Slot,
		"blockRoot": update.FinalizedHeaderBlockRoot,
	}).Info("syncing finalized header from Ethereum beacon client")

	currentSyncPeriod := h.syncer.ComputeSyncPeriodAtSlot(uint64(update.Payload.AttestedHeader.Slot))
	lastSyncedPeriod := h.syncer.ComputeSyncPeriodAtSlot(h.cache.Finalized.LastSyncedSlot)

	if lastSyncedPeriod < currentSyncPeriod {
		err = h.syncLaggingSyncCommitteePeriods(ctx, lastSyncedPeriod, currentSyncPeriod)
		if err != nil {
			return fmt.Errorf("sync lagging sync committee periods: %w", err)
		}
	}

	err = h.writer.WriteToParachainAndWatch(ctx, "EthereumBeaconClient.submit", update.Payload)
	if err != nil {
		return fmt.Errorf("write to parachain: %w", err)
	}

	lastFinalizedHeaderState, err := h.writer.GetLastFinalizedHeaderState()
	if err != nil {
		return fmt.Errorf("fetch last finalized header state: %w", err)
	}

	lastStoredHeader := lastFinalizedHeaderState.BeaconBlockRoot

	if lastStoredHeader != update.FinalizedHeaderBlockRoot {
		return ErrFinalizedHeaderNotImported
	}

	// If the finalized header import succeeded, we add it to this cache.
	h.cache.SetLastSyncedFinalizedState(update.FinalizedHeaderBlockRoot, uint64(update.Payload.FinalizedHeader.Slot))
	h.cache.AddCheckPoint(update.FinalizedHeaderBlockRoot, update.BlockRootsTree, uint64(update.Payload.FinalizedHeader.Slot))
	return nil
}

func (h *Header) SyncHeaders(ctx context.Context) error {
	err := h.SyncExecutionHeaders(ctx)
	if err != nil {
		return err
	}

	hasChanged, err := h.syncer.HasFinalizedHeaderChanged(h.cache.Finalized.LastSyncedHash)
	if err != nil {
		return err
	}

	if !hasChanged {
		return ErrFinalizedHeaderUnchanged
	}

	err = h.SyncFinalizedHeader(ctx)
	if err != nil {
		return err
	}

	return nil
}

func (h *Header) SyncExecutionHeaders(ctx context.Context) error {
	fromSlot := h.cache.LastSyncedExecutionSlot
	// SyncExecutionHeaders at least from initial checkpoint
	if fromSlot <= h.cache.InitialCheckpointSlot {
		fromSlot = h.cache.InitialCheckpointSlot
	}
	toSlot := h.cache.Finalized.LastSyncedSlot
	if fromSlot >= toSlot {
		log.WithFields(log.Fields{
			"fromSlot": fromSlot,
			"toSlot":   toSlot,
		}).Info("execution headers sync up to date with last finalized header")
		return nil
	}
	log.WithFields(log.Fields{
		"fromSlot":   fromSlot,
		"fromEpoch":  h.syncer.ComputeEpochAtSlot(fromSlot),
		"toSlot":     toSlot,
		"toEpoch":    h.syncer.ComputeEpochAtSlot(toSlot),
		"totalSlots": toSlot - fromSlot,
	}).Info("starting to back-fill headers")

	var headersToSync []scale.HeaderUpdatePayload

	// start syncing at next block after last synced block
	currentSlot := fromSlot
	headerUpdate, err := h.getNextHeaderUpdateBySlot(currentSlot)
	if err != nil {
		return fmt.Errorf("get next header update by slot with ancestry proof: %w", err)
	}
	currentSlot = uint64(headerUpdate.Header.Slot)

	for currentSlot <= toSlot {
		log.WithFields(log.Fields{
			"currentSlot": currentSlot,
		}).Info("fetching next header at slot")

		var nextHeaderUpdate scale.HeaderUpdatePayload
		if currentSlot >= toSlot {
			// Just construct an empty update so to break the loop
			nextHeaderUpdate = scale.HeaderUpdatePayload{Header: scale.BeaconHeader{Slot: types.U64(toSlot + 1)}}
		} else {
			// To get the sync witness for the current synced header. This header
			// will be used as the next update.
			nextHeaderUpdate, err = h.getNextHeaderUpdateBySlot(currentSlot)
			if err != nil {
				return fmt.Errorf("get next header update by slot with ancestry proof: %w", err)
			}
		}

		headersToSync = append(headersToSync, headerUpdate)
		// last slot to be synced, sync headers
		if currentSlot >= toSlot {
			err = h.batchSyncHeaders(ctx, headersToSync)
			if err != nil {
				return fmt.Errorf("batch sync headers failed: %w", err)
			}
		}
		headerUpdate = nextHeaderUpdate
		currentSlot = uint64(headerUpdate.Header.Slot)
	}
	// waiting for all batch calls to be executed on chain
	err = h.waitingForBatchCallFinished(toSlot)
	if err != nil {
		return err
	}
	h.cache.SetLastSyncedExecutionSlot(toSlot)
	return nil
}

func (h *Header) syncLaggingSyncCommitteePeriods(ctx context.Context, latestSyncedPeriod, currentSyncPeriod uint64) error {
	// sync for the next period
	periodsToSync := []uint64{latestSyncedPeriod + 1}

	// For initialPeriod special handling here to sync it again for nextSyncCommittee which is not included in InitCheckpoint
	if h.isInitialSyncPeriod() {
		periodsToSync = append([]uint64{latestSyncedPeriod}, periodsToSync...)
	}

	log.WithFields(log.Fields{
		"periods": periodsToSync,
	}).Info("sync committee periods to be synced")

	for _, period := range periodsToSync {
		err := h.SyncCommitteePeriodUpdate(ctx, period)
		if err != nil {
			return err
		}
	}

	// If Latency found between LastSyncedSyncCommitteePeriod and currentSyncPeriod in Ethereum beacon client
	// just return error so to exit ASAP to allow ExecutionUpdate to catch up
	lastSyncedPeriod := h.syncer.ComputeSyncPeriodAtSlot(h.cache.Finalized.LastSyncedSlot)
	if lastSyncedPeriod < currentSyncPeriod {
		return ErrSyncCommitteeLatency
	}

	return nil
}

func (h *Header) populateFinalizedCheckpoint(slot uint64) error {
	finalizedHeader, err := h.syncer.Client.GetHeaderBySlot(slot)
	if err != nil {
		return fmt.Errorf("get header by slot: %w", err)
	}

	scaleHeader, err := finalizedHeader.ToScale()
	if err != nil {
		return fmt.Errorf("header to scale: %w", err)
	}

	blockRoot, err := scaleHeader.ToSSZ().HashTreeRoot()
	if err != nil {
		return fmt.Errorf("header hash root: %w", err)
	}

	// Always check slot finalized on chain before populating checkpoint
	onChainFinalizedHeader, err := h.writer.GetFinalizedHeaderStateByBlockRoot(blockRoot)
	if err != nil {
		return err
	}
	if onChainFinalizedHeader.BeaconSlot != slot {
		return fmt.Errorf("on chain finalized header inconsistent at slot %d", slot)
	}

	blockRootsProof, err := h.syncer.GetBlockRoots(slot)
	if err != nil && !errors.Is(err, syncer.ErrBeaconStateAvailableYet) {
		return fmt.Errorf("fetch block roots: %w", err)
	}

	log.Info("populating checkpoint")

	h.cache.AddCheckPoint(blockRoot, blockRootsProof.Tree, slot)

	return nil
}

func (h *Header) populateClosestCheckpoint(slot uint64) (cache.Proof, error) {
	checkpoint, err := h.cache.GetClosestCheckpoint(slot)

	switch {
	case errors.Is(cache.FinalizedCheckPointNotAvailable, err) || errors.Is(cache.FinalizedCheckPointNotPopulated, err):
		checkpointSlot := checkpoint.Slot
		if checkpointSlot == 0 {
			checkpointSlot = h.syncer.CalculateNextCheckpointSlot(slot)
			log.WithFields(log.Fields{"calculatedCheckpointSlot": checkpointSlot}).Info("checkpoint slot not available, try with slot in next sync period instead")
		}
		err := h.populateFinalizedCheckpoint(checkpointSlot)
		if err != nil {
			return cache.Proof{}, fmt.Errorf("populate closest checkpoint: %w", err)
		}

		log.Info("populated finalized checkpoint")

		checkpoint, err = h.cache.GetClosestCheckpoint(slot)
		if err != nil {
			return cache.Proof{}, fmt.Errorf("get closest checkpoint after populating finalized header: %w", err)
		}

		log.WithFields(log.Fields{"slot": slot, "checkpoint": checkpoint}).Info("checkpoint after populating finalized header")

		return checkpoint, nil
	case err != nil:
		return cache.Proof{}, fmt.Errorf("get closest checkpoint: %w", err)
	}

	return checkpoint, nil
}

func (h *Header) getNextHeaderUpdateBySlot(slot uint64) (scale.HeaderUpdatePayload, error) {
	slot = slot + 1
	header, err := h.syncer.FindBeaconHeaderWithBlockIncluded(slot)
	if err != nil {
		return scale.HeaderUpdatePayload{}, fmt.Errorf("get next beacon header with block included: %w", err)
	}
	checkpoint, err := h.populateClosestCheckpoint(header.Slot)
	if err != nil {
		return scale.HeaderUpdatePayload{}, fmt.Errorf("populate closest checkpoint: %w", err)
	}
	blockRoot, err := header.HashTreeRoot()
	if err != nil {
		return scale.HeaderUpdatePayload{}, fmt.Errorf("header hash tree root: %w", err)
	}
	return h.syncer.GetHeaderUpdate(blockRoot, &checkpoint)
}

func (h *Header) batchSyncHeaders(ctx context.Context, headerUpdates []scale.HeaderUpdatePayload) error {
	headerUpdatesInf := make([]interface{}, len(headerUpdates))
	for i, v := range headerUpdates {
		headerUpdatesInf[i] = v
	}
	err := h.writer.BatchCall(ctx, "EthereumBeaconClient.submit_execution_header", headerUpdatesInf)
	if err != nil {
		return err
	}
	return nil
}

func (h *Header) isInitialSyncPeriod() bool {
	initialPeriod := h.syncer.ComputeSyncPeriodAtSlot(h.cache.InitialCheckpointSlot)
	lastFinalizedPeriod := h.syncer.ComputeSyncPeriodAtSlot(h.cache.Finalized.LastSyncedSlot)
	return initialPeriod == lastFinalizedPeriod
}

func (h *Header) waitingForBatchCallFinished(toSlot uint64) error {
	batchCallFinished := false
	cnt := 0
	for cnt <= 12 {
		executionHeaderState, err := h.writer.GetLastExecutionHeaderState()
		if err != nil {
			return fmt.Errorf("fetch last execution hash: %w", err)
		}
		if executionHeaderState.BeaconSlot == toSlot {
			batchCallFinished = true
			break
		}
		time.Sleep(6 * time.Second)
		cnt++
	}
	if !batchCallFinished {
		return ErrExecutionHeaderNotImported
	}
	return nil
}
