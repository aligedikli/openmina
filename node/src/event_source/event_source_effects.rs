use p2p::channels::snark::P2pChannelsSnarkAction;
use p2p::listen::{
    P2pListenClosedAction, P2pListenErrorAction, P2pListenExpiredAction, P2pListenNewAction,
};
use p2p::P2pListenEvent;

use crate::action::CheckTimeoutsAction;
use crate::block_producer::vrf_evaluator::BlockProducerVrfEvaluatorEvaluationSuccessAction;
use crate::external_snark_worker::ExternalSnarkWorkerEvent;
use crate::p2p::channels::best_tip::P2pChannelsBestTipAction;
use crate::p2p::channels::rpc::P2pChannelsRpcAction;
use crate::p2p::channels::snark_job_commitment::P2pChannelsSnarkJobCommitmentAction;
use crate::p2p::channels::{ChannelId, P2pChannelsMessageReceivedAction};
use crate::p2p::connection::incoming::{
    P2pConnectionIncomingAnswerSdpCreateErrorAction,
    P2pConnectionIncomingAnswerSdpCreateSuccessAction, P2pConnectionIncomingFinalizeErrorAction,
    P2pConnectionIncomingFinalizeSuccessAction, P2pConnectionIncomingLibp2pReceivedAction,
};
use crate::p2p::connection::outgoing::{
    P2pConnectionOutgoingAnswerRecvErrorAction, P2pConnectionOutgoingAnswerRecvSuccessAction,
    P2pConnectionOutgoingFinalizeErrorAction, P2pConnectionOutgoingFinalizeSuccessAction,
    P2pConnectionOutgoingOfferSdpCreateErrorAction,
    P2pConnectionOutgoingOfferSdpCreateSuccessAction,
};
use crate::p2p::connection::{P2pConnectionErrorResponse, P2pConnectionResponse};
use crate::p2p::disconnection::{P2pDisconnectionAction, P2pDisconnectionReason};
use crate::p2p::discovery::{
    P2pDiscoveryKademliaAddRouteAction, P2pDiscoveryKademliaFailureAction,
    P2pDiscoveryKademliaSuccessAction,
};
use crate::p2p::P2pChannelEvent;
use crate::rpc::{
    RpcActionStatsGetAction, RpcGlobalStateGetAction, RpcHealthCheckAction,
    RpcP2pConnectionIncomingInitAction, RpcP2pConnectionOutgoingInitAction, RpcPeersGetAction,
    RpcReadinessCheckAction, RpcRequest, RpcScanStateSummaryGetAction,
    RpcSnarkPoolAvailableJobsGetAction, RpcSnarkPoolJobGetAction, RpcSnarkerConfigGetAction,
    RpcSnarkerJobCommitAction, RpcSnarkerJobSpecAction, RpcSnarkersWorkersGetAction,
    RpcSyncStatsGetAction,
};
use crate::snark::block_verify::SnarkBlockVerifyAction;
use crate::snark::work_verify::SnarkWorkVerifyAction;
use crate::snark::SnarkEvent;
use crate::{ExternalSnarkWorkerAction, Service, Store};

use super::{Event, EventSourceAction, EventSourceActionWithMeta, P2pConnectionEvent, P2pEvent};

pub fn event_source_effects<S: Service>(store: &mut Store<S>, action: EventSourceActionWithMeta) {
    let (action, meta) = action.split();
    match action {
        EventSourceAction::ProcessEvents => {
            // This action gets continously called until there are no more
            // events available.
            //
            // Retrieve and process max 1024 events at a time and dispatch
            // `CheckTimeoutsAction` in between `EventSourceProcessEventsAction`
            // calls so that we make sure, that action gets called even
            // if we are continously flooded with events.
            for _ in 0..1024 {
                match store.service.next_event() {
                    Some(event) => {
                        store.dispatch(EventSourceAction::NewEvent { event });
                    }
                    None => break,
                }
            }
            store.dispatch(CheckTimeoutsAction {});
        }
        // "Translate" event into the corresponding action and dispatch it.
        EventSourceAction::NewEvent { event } => match event {
            Event::P2p(e) => match e {
                P2pEvent::Listen(e) => match e {
                    P2pListenEvent::NewListenAddr { listener_id, addr } => {
                        store.dispatch(P2pListenNewAction { listener_id, addr });
                    }
                    P2pListenEvent::ExpiredListenAddr { listener_id, addr } => {
                        store.dispatch(P2pListenExpiredAction { listener_id, addr });
                    }
                    P2pListenEvent::ListenerError { listener_id, error } => {
                        store.dispatch(P2pListenErrorAction { listener_id, error });
                    }
                    P2pListenEvent::ListenerClosed { listener_id, error } => {
                        store.dispatch(P2pListenClosedAction { listener_id, error });
                    }
                },
                P2pEvent::Connection(e) => match e {
                    P2pConnectionEvent::OfferSdpReady(peer_id, res) => match res {
                        Err(error) => {
                            store.dispatch(P2pConnectionOutgoingOfferSdpCreateErrorAction {
                                peer_id,
                                error,
                            });
                        }
                        Ok(sdp) => {
                            store.dispatch(P2pConnectionOutgoingOfferSdpCreateSuccessAction {
                                peer_id,
                                sdp,
                            });
                        }
                    },
                    P2pConnectionEvent::AnswerSdpReady(peer_id, res) => match res {
                        Err(error) => {
                            store.dispatch(P2pConnectionIncomingAnswerSdpCreateErrorAction {
                                peer_id,
                                error,
                            });
                        }
                        Ok(sdp) => {
                            store.dispatch(P2pConnectionIncomingAnswerSdpCreateSuccessAction {
                                peer_id,
                                sdp,
                            });
                        }
                    },
                    P2pConnectionEvent::AnswerReceived(peer_id, res) => match res {
                        P2pConnectionResponse::Accepted(answer) => {
                            store.dispatch(P2pConnectionOutgoingAnswerRecvSuccessAction {
                                peer_id,
                                answer,
                            });
                        }
                        P2pConnectionResponse::Rejected(reason) => {
                            store.dispatch(P2pConnectionOutgoingAnswerRecvErrorAction {
                                peer_id,
                                error: P2pConnectionErrorResponse::Rejected(reason),
                            });
                        }
                        P2pConnectionResponse::InternalError => {
                            store.dispatch(P2pConnectionOutgoingAnswerRecvErrorAction {
                                peer_id,
                                error: P2pConnectionErrorResponse::InternalError,
                            });
                        }
                    },
                    P2pConnectionEvent::Finalized(peer_id, res) => match res {
                        Err(error) => {
                            store.dispatch(P2pConnectionOutgoingFinalizeErrorAction {
                                peer_id,
                                error: error.clone(),
                            });
                            store.dispatch(P2pConnectionIncomingFinalizeErrorAction {
                                peer_id,
                                error,
                            });
                        }
                        Ok(_) => {
                            let _ = store
                                .dispatch(P2pConnectionOutgoingFinalizeSuccessAction { peer_id })
                                || store.dispatch(P2pConnectionIncomingFinalizeSuccessAction {
                                    peer_id,
                                })
                                || store.dispatch(P2pConnectionIncomingLibp2pReceivedAction {
                                    peer_id,
                                });
                        }
                    },
                    P2pConnectionEvent::Closed(peer_id) => {
                        store.dispatch(P2pDisconnectionAction::Finish { peer_id });
                    }
                },
                P2pEvent::Channel(e) => match e {
                    P2pChannelEvent::Opened(peer_id, chan_id, res) => match res {
                        Err(err) => {
                            openmina_core::log::warn!(meta.time(); kind = "P2pChannelEvent::Opened", peer_id = peer_id.to_string(), error = err);
                            // TODO(binier): dispatch error action.
                        }
                        Ok(_) => match chan_id {
                            ChannelId::BestTipPropagation => {
                                // TODO(binier): maybe dispatch success and then ready.
                                store.dispatch(P2pChannelsBestTipAction::Ready { peer_id });
                            }
                            ChannelId::SnarkPropagation => {
                                // TODO(binier): maybe dispatch success and then ready.
                                store.dispatch(P2pChannelsSnarkAction::Ready { peer_id });
                            }
                            ChannelId::SnarkJobCommitmentPropagation => {
                                // TODO(binier): maybe dispatch success and then ready.
                                store.dispatch(P2pChannelsSnarkJobCommitmentAction::Ready {
                                    peer_id,
                                });
                            }
                            ChannelId::Rpc => {
                                // TODO(binier): maybe dispatch success and then ready.
                                store.dispatch(P2pChannelsRpcAction::Ready { peer_id });
                            }
                        },
                    },
                    P2pChannelEvent::Sent(peer_id, _, _, res) => {
                        if let Err(err) = res {
                            let reason = P2pDisconnectionReason::P2pChannelSendFailed(err);
                            store.dispatch(P2pDisconnectionAction::Init { peer_id, reason });
                        }
                    }
                    P2pChannelEvent::Received(peer_id, res) => match res {
                        Err(err) => {
                            let reason = P2pDisconnectionReason::P2pChannelReceiveFailed(err);
                            store.dispatch(P2pDisconnectionAction::Init { peer_id, reason });
                        }
                        Ok(message) => {
                            store.dispatch(P2pChannelsMessageReceivedAction { peer_id, message });
                        }
                    },
                    P2pChannelEvent::Libp2pSnarkReceived(peer_id, snark, nonce) => {
                        store.dispatch(P2pChannelsSnarkAction::Libp2pReceived {
                            peer_id,
                            snark,
                            nonce,
                        });
                    }
                    P2pChannelEvent::Closed(peer_id, chan_id) => {
                        let reason = P2pDisconnectionReason::P2pChannelClosed(chan_id);
                        store.dispatch(P2pDisconnectionAction::Init { peer_id, reason });
                    }
                },
                #[cfg(not(target_arch = "wasm32"))]
                P2pEvent::Libp2pIdentify(..) => {}
                P2pEvent::Discovery(p2p::P2pDiscoveryEvent::Ready) => {}
                P2pEvent::Discovery(p2p::P2pDiscoveryEvent::DidFindPeers(peers)) => {
                    store.dispatch(P2pDiscoveryKademliaSuccessAction { peers });
                }
                P2pEvent::Discovery(p2p::P2pDiscoveryEvent::DidFindPeersError(description)) => {
                    store.dispatch(P2pDiscoveryKademliaFailureAction { description });
                }
                P2pEvent::Discovery(p2p::P2pDiscoveryEvent::AddRoute(peer_id, addresses)) => {
                    store.dispatch(P2pDiscoveryKademliaAddRouteAction { peer_id, addresses });
                }
            },
            Event::Snark(event) => match event {
                SnarkEvent::BlockVerify(req_id, result) => match result {
                    Err(error) => {
                        store.dispatch(SnarkBlockVerifyAction::Error { req_id, error });
                    }
                    Ok(()) => {
                        store.dispatch(SnarkBlockVerifyAction::Success { req_id });
                    }
                },
                SnarkEvent::WorkVerify(req_id, result) => match result {
                    Err(error) => {
                        store.dispatch(SnarkWorkVerifyAction::Error { req_id, error });
                    }
                    Ok(()) => {
                        store.dispatch(SnarkWorkVerifyAction::Success { req_id });
                    }
                },
            },
            Event::Rpc(rpc_id, e) => match e {
                RpcRequest::StateGet => {
                    store.dispatch(RpcGlobalStateGetAction { rpc_id });
                }
                RpcRequest::ActionStatsGet(query) => {
                    store.dispatch(RpcActionStatsGetAction { rpc_id, query });
                }
                RpcRequest::SyncStatsGet(query) => {
                    store.dispatch(RpcSyncStatsGetAction { rpc_id, query });
                }
                RpcRequest::PeersGet => {
                    store.dispatch(RpcPeersGetAction { rpc_id });
                }
                RpcRequest::P2pConnectionOutgoing(opts) => {
                    store.dispatch(RpcP2pConnectionOutgoingInitAction { rpc_id, opts });
                }
                RpcRequest::P2pConnectionIncoming(opts) => {
                    store.dispatch(RpcP2pConnectionIncomingInitAction {
                        rpc_id,
                        opts: opts.clone(),
                    });
                }
                RpcRequest::ScanStateSummaryGet(query) => {
                    store.dispatch(RpcScanStateSummaryGetAction { rpc_id, query });
                }
                RpcRequest::SnarkPoolGet => {
                    store.dispatch(RpcSnarkPoolAvailableJobsGetAction { rpc_id });
                }
                RpcRequest::SnarkPoolJobGet { job_id } => {
                    store.dispatch(RpcSnarkPoolJobGetAction { rpc_id, job_id });
                }
                RpcRequest::SnarkerConfig => {
                    store.dispatch(RpcSnarkerConfigGetAction { rpc_id });
                }
                RpcRequest::SnarkerJobCommit { job_id } => {
                    store.dispatch(RpcSnarkerJobCommitAction { rpc_id, job_id });
                }
                RpcRequest::SnarkerJobSpec { job_id } => {
                    store.dispatch(RpcSnarkerJobSpecAction { rpc_id, job_id });
                }
                RpcRequest::SnarkerWorkers => {
                    store.dispatch(RpcSnarkersWorkersGetAction { rpc_id });
                }
                RpcRequest::HealthCheck => {
                    store.dispatch(RpcHealthCheckAction { rpc_id });
                }
                RpcRequest::ReadinessCheck => {
                    store.dispatch(RpcReadinessCheckAction { rpc_id });
                }
            },
            Event::ExternalSnarkWorker(e) => match e {
                ExternalSnarkWorkerEvent::Started => {
                    store.dispatch(ExternalSnarkWorkerAction::Started);
                }
                ExternalSnarkWorkerEvent::Killed => {
                    store.dispatch(ExternalSnarkWorkerAction::Killed);
                }
                ExternalSnarkWorkerEvent::WorkResult(result) => {
                    store.dispatch(ExternalSnarkWorkerAction::WorkResult { result });
                }
                ExternalSnarkWorkerEvent::WorkError(error) => {
                    store.dispatch(ExternalSnarkWorkerAction::WorkError { error });
                }
                ExternalSnarkWorkerEvent::WorkCancelled => {
                    store.dispatch(ExternalSnarkWorkerAction::WorkCancelled);
                }
                ExternalSnarkWorkerEvent::Error(error) => {
                    store.dispatch(ExternalSnarkWorkerAction::Error {
                        error,
                        permanent: false,
                    });
                }
            },
            Event::BlockProducerEvent(e) => match e {
                crate::block_producer::BlockProducerEvent::VrfEvaluator(vrf_e) => match vrf_e {
                    crate::block_producer::BlockProducerVrfEvaluatorEvent::Evaluated(
                        vrf_output_with_hash,
                    ) => {
                        store.dispatch(BlockProducerVrfEvaluatorEvaluationSuccessAction {
                            vrf_output: vrf_output_with_hash.evaluation_result,
                            staking_ledger_hash: vrf_output_with_hash.staking_ledger_hash,
                        });
                    }
                },
            },
        },
        EventSourceAction::WaitTimeout => {
            store.dispatch(CheckTimeoutsAction {});
        }
        EventSourceAction::WaitForEvents => {}
    }
}
