use std::sync::Arc;

use mina_p2p_messages::v2::{LedgerHash, MinaBaseAccountBinableArgStableV2};
use serde::{Deserialize, Serialize};

use crate::ledger::{LedgerAddress, LEDGER_DEPTH};
use crate::p2p::channels::rpc::{P2pRpcId, StagedLedgerAuxAndPendingCoinbases};
use crate::p2p::PeerId;

use super::{
    PeerLedgerQueryError, PeerLedgerQueryResponse, PeerRpcState, PeerStagedLedgerReconstructState,
    TransitionFrontierSyncLedgerState,
};

pub type TransitionFrontierSyncLedgerActionWithMeta =
    redux::ActionWithMeta<TransitionFrontierSyncLedgerAction>;
pub type TransitionFrontierSyncLedgerActionWithMetaRef<'a> =
    redux::ActionWithMeta<&'a TransitionFrontierSyncLedgerAction>;

#[derive(derive_more::From, Serialize, Deserialize, Debug, Clone)]
pub enum TransitionFrontierSyncLedgerAction {
    Init(TransitionFrontierSyncLedgerInitAction),
    SnarkedPending(TransitionFrontierSyncLedgerSnarkedPendingAction),
    SnarkedPeersQuery(TransitionFrontierSyncLedgerSnarkedPeersQueryAction),
    SnarkedPeerQueryInit(TransitionFrontierSyncLedgerSnarkedPeerQueryInitAction),
    SnarkedPeerQueryPending(TransitionFrontierSyncLedgerSnarkedPeerQueryPendingAction),
    SnarkedPeerQueryRetry(TransitionFrontierSyncLedgerSnarkedPeerQueryRetryAction),
    SnarkedPeerQueryError(TransitionFrontierSyncLedgerSnarkedPeerQueryErrorAction),
    SnarkedPeerQuerySuccess(TransitionFrontierSyncLedgerSnarkedPeerQuerySuccessAction),
    SnarkedChildHashesReceived(TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction),
    SnarkedChildAccountsReceived(TransitionFrontierSyncLedgerSnarkedChildAccountsReceivedAction),
    SnarkedSuccess(TransitionFrontierSyncLedgerSnarkedSuccessAction),
    StagedReconstructPending(TransitionFrontierSyncLedgerStagedReconstructPendingAction),
    StagedPartsFetchInit(TransitionFrontierSyncLedgerStagedPartsFetchInitAction),
    StagedPartsFetchPending(TransitionFrontierSyncLedgerStagedPartsFetchPendingAction),
    StagedPartsFetchError(TransitionFrontierSyncLedgerStagedPartsFetchErrorAction),
    StagedPartsFetchSuccess(TransitionFrontierSyncLedgerStagedPartsFetchSuccessAction),
    StagedPartsApplyInit(TransitionFrontierSyncLedgerStagedPartsApplyInitAction),
    StagedPartsApplySuccess(TransitionFrontierSyncLedgerStagedPartsApplySuccessAction),
    StagedReconstructSuccess(TransitionFrontierSyncLedgerStagedReconstructSuccessAction),
    Success(TransitionFrontierSyncLedgerSuccessAction),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerInitAction {}

impl redux::EnablingCondition<crate::State> for TransitionFrontierSyncLedgerInitAction {
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| {
                matches!(s, TransitionFrontierSyncLedgerState::Init { .. })
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedPendingAction {}

impl redux::EnablingCondition<crate::State> for TransitionFrontierSyncLedgerSnarkedPendingAction {
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| {
                matches!(s, TransitionFrontierSyncLedgerState::Init { .. })
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedPeersQueryAction {}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedPeersQueryAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        let peers_available = state
            .p2p
            .ready_peers_iter()
            .any(|(_, p)| p.channels.rpc.can_send_request());
        peers_available
            && state
                .transition_frontier
                .sync
                .root_ledger()
                .map_or(false, |s| {
                    s.snarked_ledger_sync_next().is_some()
                        || s.snarked_ledger_sync_retry_iter().next().is_some()
                })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedPeerQueryInitAction {
    pub address: LedgerAddress,
    pub peer_id: PeerId,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedPeerQueryInitAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        let check_next_addr =
            state
                .transition_frontier
                .sync
                .root_ledger()
                .map_or(false, |s| match s {
                    TransitionFrontierSyncLedgerState::SnarkedPending {
                        pending,
                        next_addr,
                        ..
                    } => next_addr.as_ref().map_or(false, |next_addr| {
                        next_addr == &self.address
                            && (next_addr.to_index().0 != 0 || pending.is_empty())
                    }),
                    _ => false,
                });

        let check_peer_available = state
            .p2p
            .get_ready_peer(&self.peer_id)
            .and_then(|p| {
                let sync_best_tip = state.transition_frontier.sync.best_tip()?;
                let peer_best_tip = p.best_tip.as_ref()?;
                Some(p).filter(|_| sync_best_tip.hash == peer_best_tip.hash)
            })
            .map_or(false, |p| p.channels.rpc.can_send_request());
        check_next_addr && check_peer_available
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedPeerQueryRetryAction {
    pub address: LedgerAddress,
    pub peer_id: PeerId,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedPeerQueryRetryAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        let check_next_addr = state
            .transition_frontier
            .sync
            .root_ledger()
            .and_then(|s| s.snarked_ledger_sync_retry_iter().next())
            .map_or(false, |addr| addr == self.address);

        let check_peer_available = state
            .p2p
            .get_ready_peer(&self.peer_id)
            .and_then(|p| {
                let sync_best_tip = state.transition_frontier.sync.best_tip()?;
                let peer_best_tip = p.best_tip.as_ref()?;
                Some(p).filter(|_| sync_best_tip.hash == peer_best_tip.hash)
            })
            .map_or(false, |p| p.channels.rpc.can_send_request());
        check_next_addr && check_peer_available
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedPeerQueryPendingAction {
    pub address: LedgerAddress,
    pub peer_id: PeerId,
    pub rpc_id: P2pRpcId,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedPeerQueryPendingAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| match s {
                TransitionFrontierSyncLedgerState::SnarkedPending { pending, .. } => pending
                    .iter()
                    .filter_map(|(_, query_state)| query_state.attempts.get(&self.peer_id))
                    .any(|peer_rpc_state| matches!(peer_rpc_state, PeerRpcState::Init { .. })),
                _ => false,
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedPeerQueryErrorAction {
    pub peer_id: PeerId,
    pub rpc_id: P2pRpcId,
    pub error: PeerLedgerQueryError,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedPeerQueryErrorAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| {
                s.snarked_ledger_peer_query_get(&self.peer_id, self.rpc_id)
                    .and_then(|(_, s)| s.attempts.get(&self.peer_id))
                    .map_or(false, |s| matches!(s, PeerRpcState::Pending { .. }))
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedPeerQuerySuccessAction {
    pub peer_id: PeerId,
    pub rpc_id: P2pRpcId,
    pub response: PeerLedgerQueryResponse,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedPeerQuerySuccessAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| {
                // TODO(binier): check if expected response
                // kind is correct.
                s.snarked_ledger_peer_query_get(&self.peer_id, self.rpc_id)
                    .and_then(|(_, s)| s.attempts.get(&self.peer_id))
                    .map_or(false, |s| matches!(s, PeerRpcState::Pending { .. }))
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction {
    pub address: LedgerAddress,
    pub hashes: (LedgerHash, LedgerHash),
    pub sender: PeerId,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        self.address.length() < LEDGER_DEPTH - 1
            && state
                .transition_frontier
                .sync
                .root_ledger()
                .and_then(|s| match s {
                    TransitionFrontierSyncLedgerState::SnarkedPending { pending, .. } => {
                        pending.get(&self.address)
                    }
                    _ => None,
                })
                .and_then(|s| s.attempts.get(&self.sender))
                .map_or(false, |s| s.is_success())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedChildAccountsReceivedAction {
    pub address: LedgerAddress,
    pub accounts: Vec<MinaBaseAccountBinableArgStableV2>,
    pub sender: PeerId,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerSnarkedChildAccountsReceivedAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .and_then(|s| match s {
                TransitionFrontierSyncLedgerState::SnarkedPending { pending, .. } => {
                    pending.get(&self.address)
                }
                _ => None,
            })
            .and_then(|s| s.attempts.get(&self.sender))
            // TODO(binier): check if expected response
            // kind is correct.
            .map_or(false, |s| s.is_success())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSnarkedSuccessAction {}

impl redux::EnablingCondition<crate::State> for TransitionFrontierSyncLedgerSnarkedSuccessAction {
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| match s {
                TransitionFrontierSyncLedgerState::SnarkedPending {
                    pending, next_addr, ..
                } => next_addr.is_none() && pending.is_empty(),
                _ => false,
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedReconstructPendingAction {}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedReconstructPendingAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| {
                matches!(s, TransitionFrontierSyncLedgerState::SnarkedSuccess { .. })
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedPartsFetchInitAction {}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedPartsFetchInitAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        let Some(root_ledger) = state.transition_frontier.sync.root_ledger() else { return false };

        let iter = state.p2p.ready_rpc_peers_iter();
        root_ledger
            .staged_ledger_reconstruct_filter_available_peers(iter)
            .next()
            .is_some()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedPartsFetchPendingAction {
    pub peer_id: PeerId,
    pub rpc_id: P2pRpcId,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedPartsFetchPendingAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| {
                matches!(
                    s,
                    TransitionFrontierSyncLedgerState::StagedReconstructPending { .. }
                )
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedPartsFetchErrorAction {
    pub peer_id: PeerId,
    pub rpc_id: P2pRpcId,
    pub error: PeerLedgerQueryError,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedPartsFetchErrorAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .and_then(|s| match s {
                TransitionFrontierSyncLedgerState::StagedReconstructPending {
                    attempts, ..
                } => attempts.get(&self.peer_id),
                _ => None,
            })
            .and_then(|s| s.fetch_pending_rpc_id())
            .map_or(false, |rpc_id| rpc_id == self.rpc_id)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedPartsFetchSuccessAction {
    pub peer_id: PeerId,
    pub rpc_id: P2pRpcId,
    pub parts: Arc<StagedLedgerAuxAndPendingCoinbases>,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedPartsFetchSuccessAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .and_then(|s| match s {
                TransitionFrontierSyncLedgerState::StagedReconstructPending {
                    attempts, ..
                } => attempts.get(&self.peer_id),
                _ => None,
            })
            .and_then(|s| s.fetch_pending_rpc_id())
            .map_or(false, |rpc_id| rpc_id == self.rpc_id)
    }
}
// TODO(binier): validate staged ledger hash in fetched parts.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedPartsApplyInitAction {
    pub sender: PeerId,
    pub parts: Arc<StagedLedgerAuxAndPendingCoinbases>,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedPartsApplyInitAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .and_then(|s| match s {
                TransitionFrontierSyncLedgerState::StagedReconstructPending {
                    attempts, ..
                } => attempts.get(&self.sender),
                _ => None,
            })
            .map_or(false, |s| match s {
                PeerStagedLedgerReconstructState::PartsFetchSuccess { parts, .. } => {
                    Arc::ptr_eq(parts, &self.parts)
                }
                _ => false,
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedPartsApplySuccessAction {
    pub sender: PeerId,
}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedPartsApplySuccessAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .and_then(|s| match s {
                TransitionFrontierSyncLedgerState::StagedReconstructPending {
                    attempts, ..
                } => attempts.get(&self.sender),
                _ => None,
            })
            .map_or(false, |s| s.is_fetch_success())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerStagedReconstructSuccessAction {}

impl redux::EnablingCondition<crate::State>
    for TransitionFrontierSyncLedgerStagedReconstructSuccessAction
{
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| match s {
                TransitionFrontierSyncLedgerState::StagedReconstructPending {
                    attempts, ..
                } => attempts.iter().any(|(_, s)| s.is_apply_success()),
                _ => false,
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransitionFrontierSyncLedgerSuccessAction {}

impl redux::EnablingCondition<crate::State> for TransitionFrontierSyncLedgerSuccessAction {
    fn is_enabled(&self, state: &crate::State) -> bool {
        state
            .transition_frontier
            .sync
            .root_ledger()
            .map_or(false, |s| {
                matches!(
                    s,
                    TransitionFrontierSyncLedgerState::StagedReconstructSuccess { .. }
                )
            })
    }
}

use crate::transition_frontier::sync::TransitionFrontierSyncAction;
use crate::transition_frontier::TransitionFrontierAction;

macro_rules! impl_into_global_action {
    ($a:ty) => {
        impl From<$a> for crate::Action {
            fn from(value: $a) -> Self {
                Self::TransitionFrontier(TransitionFrontierAction::Sync(
                    TransitionFrontierSyncAction::Ledger(value.into()),
                ))
            }
        }
    };
}

impl_into_global_action!(TransitionFrontierSyncLedgerInitAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedPendingAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedPeersQueryAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedPeerQueryInitAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedPeerQueryPendingAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedPeerQueryErrorAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedPeerQueryRetryAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedPeerQuerySuccessAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedChildAccountsReceivedAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSnarkedSuccessAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedReconstructPendingAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedPartsFetchInitAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedPartsFetchPendingAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedPartsFetchErrorAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedPartsFetchSuccessAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedPartsApplyInitAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedPartsApplySuccessAction);
impl_into_global_action!(TransitionFrontierSyncLedgerStagedReconstructSuccessAction);
impl_into_global_action!(TransitionFrontierSyncLedgerSuccessAction);
