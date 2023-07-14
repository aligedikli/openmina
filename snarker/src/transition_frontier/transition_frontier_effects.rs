use redux::Timestamp;

use crate::ledger::LEDGER_DEPTH;
use crate::stats::sync::SyncingLedger;
use crate::Store;

use super::sync::ledger::TransitionFrontierSyncLedgerAction;
use super::sync::{
    TransitionFrontierSyncAction, TransitionFrontierSyncLedgerRootSuccessAction,
    TransitionFrontierSyncState,
};
use super::{
    TransitionFrontierAction, TransitionFrontierActionWithMeta, TransitionFrontierSyncedAction,
};

pub fn transition_frontier_effects<S: crate::Service>(
    store: &mut Store<S>,
    action: TransitionFrontierActionWithMeta,
) {
    let (action, meta) = action.split();

    match action {
        TransitionFrontierAction::Sync(a) => match a {
            TransitionFrontierSyncAction::Init(a) => {
                if let Some(stats) = store.service.stats() {
                    stats.new_sync_target(meta.time(), &a.best_tip);
                    if let TransitionFrontierSyncState::BlocksPending { chain, .. } =
                        &store.state.get().transition_frontier.sync
                    {
                        stats.syncing_blocks_init(chain);
                    }
                }
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BestTipUpdate(a) => {
                if let Some(stats) = store.service.stats() {
                    stats.new_sync_target(meta.time(), &a.best_tip);
                    if let TransitionFrontierSyncState::BlocksPending { chain, .. } =
                        &store.state.get().transition_frontier.sync
                    {
                        stats.syncing_blocks_init(chain);
                    }
                }
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::LedgerRootPending(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::LedgerRootSuccess(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksPending(a) => {
                if let Some(stats) = store.service.stats() {
                    if let TransitionFrontierSyncState::BlocksPending { chain, .. } =
                        &store.state.get().transition_frontier.sync
                    {
                        stats.syncing_blocks_init(chain);
                    }
                }
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksPeersQuery(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksPeerQueryInit(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksPeerQueryInit(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksPeerQueryRetry(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksPeerQueryPending(a) => {
                if let Some(stats) = store.service.stats() {
                    if let Some(state) = store
                        .state
                        .get()
                        .transition_frontier
                        .sync
                        .block_state(&a.hash)
                    {
                        stats.syncing_block_update(state);
                    }
                }
            }
            TransitionFrontierSyncAction::BlocksPeerQueryError(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksPeerQuerySuccess(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksFetchSuccess(a) => {
                if let Some(stats) = store.service.stats() {
                    if let Some(state) = store
                        .state
                        .get()
                        .transition_frontier
                        .sync
                        .block_state(&a.hash)
                    {
                        stats.syncing_block_update(state);
                    }
                }
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksNextApplyInit(a) => {
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksNextApplyPending(a) => {
                if let Some(stats) = store.service.stats() {
                    if let Some(state) = store
                        .state
                        .get()
                        .transition_frontier
                        .sync
                        .block_state(&a.hash)
                    {
                        stats.syncing_block_update(state);
                    }
                }
            }
            TransitionFrontierSyncAction::BlocksNextApplySuccess(a) => {
                if let Some(stats) = store.service.stats() {
                    if let Some(state) = store
                        .state
                        .get()
                        .transition_frontier
                        .sync
                        .block_state(&a.hash)
                    {
                        stats.syncing_block_update(state);
                    }
                }
                a.effects(&meta, store);
            }
            TransitionFrontierSyncAction::BlocksSuccess(a) => {
                let sync = &store.state.get().transition_frontier.sync;
                let TransitionFrontierSyncState::BlocksSuccess { chain, .. } = sync else { return };
                let Some(root_block) = chain.first() else { return };
                let ledgers_to_keep = chain
                    .iter()
                    .flat_map(|b| [b.snarked_ledger_hash(), b.staged_ledger_hash()])
                    .cloned()
                    .collect();
                store.service.commit(ledgers_to_keep, root_block);
                store.dispatch(TransitionFrontierSyncedAction {});
            }
            TransitionFrontierSyncAction::Ledger(a) => match a {
                TransitionFrontierSyncLedgerAction::Init(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedPending(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedPeersQuery(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedPeerQueryInit(action) => {
                    if let Some(stats) = store.service().stats() {
                        let (start, end) = (meta.time(), meta.time());
                        if action.address.length() < LEDGER_DEPTH - 1 {
                            stats.syncing_ledger(SyncingLedger::FetchHashes { start, end });
                        } else {
                            stats.syncing_ledger(SyncingLedger::FetchAccounts { start, end });
                        }
                    }
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedPeerQueryRetry(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedPeerQueryPending(_) => {}
                TransitionFrontierSyncLedgerAction::SnarkedPeerQueryError(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedPeerQuerySuccess(action) => {
                    if let Some(stats) = store.service.stats() {
                        if let Some((start, end)) = store
                            .state
                            .get()
                            .transition_frontier
                            .sync
                            .root_ledger()
                            .and_then(|s| {
                                s.snarked_ledger_peer_query_get(&action.peer_id, action.rpc_id)
                            })
                            .map(|(_, s)| (s.time, meta.time()))
                        {
                            if action.response.is_child_hashes() {
                                stats.syncing_ledger(SyncingLedger::FetchHashes { start, end });
                            } else if action.response.is_child_accounts() {
                                stats.syncing_ledger(SyncingLedger::FetchAccounts { start, end });
                            }
                        }
                    }
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedChildHashesReceived(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedChildAccountsReceived(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::SnarkedSuccess(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::StagedReconstructPending(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::StagedPartsFetchInit(action) => {
                    if let Some(stats) = store.service().stats() {
                        let (start, end) = (meta.time(), None);
                        stats.syncing_ledger(SyncingLedger::FetchParts { start, end });
                    }
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::StagedPartsFetchPending(_) => {}
                TransitionFrontierSyncLedgerAction::StagedPartsFetchError(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::StagedPartsFetchSuccess(action) => {
                    if let Some(stats) = store.service().stats() {
                        let (start, end) = (Timestamp::ZERO, Some(meta.time()));
                        stats.syncing_ledger(SyncingLedger::FetchParts { start, end });
                    }
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::StagedPartsApplyInit(action) => {
                    if let Some(stats) = store.service().stats() {
                        let (start, end) = (meta.time(), None);
                        stats.syncing_ledger(SyncingLedger::ApplyParts { start, end });
                    }
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::StagedPartsApplySuccess(action) => {
                    if let Some(stats) = store.service().stats() {
                        let (start, end) = (Timestamp::ZERO, Some(meta.time()));
                        stats.syncing_ledger(SyncingLedger::ApplyParts { start, end });
                    }
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::StagedReconstructSuccess(action) => {
                    action.effects(&meta, store);
                }
                TransitionFrontierSyncLedgerAction::Success(_) => {
                    store.dispatch(TransitionFrontierSyncLedgerRootSuccessAction {});
                }
            },
        },
        TransitionFrontierAction::Synced(_) => {
            let Some(best_tip) = store.state.get().transition_frontier.best_tip() else { return };
            if let Some(stats) = store.service.stats() {
                stats.new_best_tip(meta.time(), best_tip);
            }
            // TODO(binier): publish new best tip
        }
    }
}
