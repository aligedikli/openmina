use serde::{Deserialize, Serialize};
use shared::requests::RpcId;

use super::incoming::{P2pConnectionIncomingInitOpts, P2pConnectionIncomingState};
use super::outgoing::{P2pConnectionOutgoingInitOpts, P2pConnectionOutgoingState};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum P2pConnectionState {
    Outgoing(P2pConnectionOutgoingState),
    Incoming(P2pConnectionIncomingState),
}

impl P2pConnectionState {
    pub fn outgoing_init(opts: &P2pConnectionOutgoingInitOpts) -> Self {
        Self::Outgoing(P2pConnectionOutgoingState::Init {
            time: redux::Timestamp::ZERO,
            opts: opts.clone(),
            rpc_id: None,
        })
    }

    pub fn incoming_init(opts: &P2pConnectionIncomingInitOpts) -> Self {
        Self::Incoming(P2pConnectionIncomingState::Init {
            time: redux::Timestamp::ZERO,
            signaling: opts.signaling.clone(),
            offer: opts.offer.clone(),
            rpc_id: None,
        })
    }

    pub fn rpc_id(&self) -> Option<RpcId> {
        match self {
            Self::Outgoing(v) => v.rpc_id(),
            Self::Incoming(v) => v.rpc_id(),
        }
    }

    pub fn is_error(&self) -> bool {
        match self {
            Self::Outgoing(P2pConnectionOutgoingState::Error { .. }) => true,
            Self::Outgoing(_) => false,
            Self::Incoming(P2pConnectionIncomingState::Error { .. }) => true,
            Self::Incoming(_) => false,
        }
    }

    pub fn is_success(&self) -> bool {
        match self {
            Self::Outgoing(P2pConnectionOutgoingState::Success { .. }) => true,
            Self::Outgoing(_) => false,
            Self::Incoming(P2pConnectionIncomingState::Success { .. }) => true,
            Self::Incoming(_) => false,
        }
    }
}
