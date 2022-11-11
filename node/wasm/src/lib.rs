use std::str::FromStr;
use std::time::Duration;

use gloo_utils::format::JsValueSerdeExt;
use libp2p::futures::channel::{mpsc, oneshot};
use libp2p::futures::select_biased;
use libp2p::futures::FutureExt;
use libp2p::futures::{SinkExt, StreamExt};
use libp2p::multiaddr::{Multiaddr, Protocol as MultiaddrProtocol};
use mina_signer::{NetworkId, PubKey, Signer};
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use lib::event_source::{
    Event, EventSourceProcessEventsAction, EventSourceWaitForEventsAction,
    EventSourceWaitTimeoutAction,
};
use lib::p2p::connection::outgoing::{
    P2pConnectionOutgoingInitAction, P2pConnectionOutgoingInitOpts,
};
use lib::p2p::pubsub::{GossipNetMessageV1, PubsubTopic};
use lib::p2p::PeerId;
use lib::rpc::RpcRequest;

mod service;
use service::libp2p::Libp2pService;
use service::rpc::{
    RpcP2pConnectionOutgoingResponse, RpcP2pPubsubPublishResponse, RpcService, RpcStateGetResponse,
};
pub use service::NodeWasmService;

mod transaction;
pub use transaction::Transaction;

pub type Store = lib::Store<NodeWasmService>;
pub type Node = lib::Node<NodeWasmService>;

fn keypair_from_bs58check_secret_key(encoded_sec: &str) -> Result<mina_signer::Keypair, JsValue> {
    mina_signer::Keypair::from_hex(encoded_sec)
        .map_err(|err| format!("Invalid Private Key: {}", err).into())
}

/// Runs endless loop.
///
/// Doesn't exit.
pub async fn run(mut node: Node) {
    let target_peer_id = "QmegiCDEULhpyW55B2qQNMSURWBKSR72445DS6JgQsfkPj";
    let target_node_addr = format!(
        "/dns4/webrtc.minasync.com/tcp/443/http/p2p-webrtc-direct/p2p/{}",
        target_peer_id
    );
    node.store_mut().dispatch(P2pConnectionOutgoingInitAction {
        opts: P2pConnectionOutgoingInitOpts {
            peer_id: target_peer_id.parse().unwrap(),
            addrs: vec![target_node_addr.parse().unwrap()],
        },
        rpc_id: None,
    });
    loop {
        let service = &mut node.store_mut().service;
        let wait_for_events = service.event_source_receiver.wait_for_events();
        let wasm_rpc_req_fut = service.rpc.wasm_req_receiver().next().then(|res| async {
            // TODO(binier): optimize maybe to not check it all the time.
            match res {
                Some(v) => v,
                None => std::future::pending().await,
            }
        });
        let timeout = wasm_timer::Delay::new(Duration::from_millis(1000));

        select_biased! {
            _ = wait_for_events.fuse() => {
                while node.store_mut().service.event_source_receiver.has_next() {
                    node.store_mut().dispatch(EventSourceProcessEventsAction {});
                }
                node.store_mut().dispatch(EventSourceWaitForEventsAction {});
            }
            req = wasm_rpc_req_fut.fuse() => {
                node.store_mut().service.process_wasm_rpc_request(req);
            }
            _ = timeout.fuse() => {
                node.store_mut().dispatch(EventSourceWaitTimeoutAction {});
            }
        }
    }
}

#[wasm_bindgen(js_name = start)]
pub async fn wasm_start() -> Result<JsHandle, JsValue> {
    wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    let (tx, rx) = mpsc::channel(1024);

    let service = NodeWasmService {
        event_source_sender: tx.clone(),
        event_source_receiver: rx.into(),
        libp2p: Libp2pService::run(tx.clone()).await,
        rpc: RpcService::new(),
    };
    let state = lib::State::new();
    let mut node = lib::Node::new(state, service);
    let rpc_sender = node.store_mut().service.wasm_rpc_req_sender().clone();

    spawn_local(run(node));
    Ok(JsHandle {
        sender: tx,
        rpc_sender,
        signer: Box::new(mina_signer::create_legacy(NetworkId::TESTNET)),
    })
}

pub struct WasmRpcRequest {
    pub req: RpcRequest,
    pub responder: Box<dyn std::any::Any>,
}

#[wasm_bindgen]
pub struct JsHandle {
    sender: mpsc::Sender<Event>,
    rpc_sender: mpsc::Sender<WasmRpcRequest>,

    signer: Box<dyn Signer<Transaction>>,
}

impl JsHandle {
    async fn rpc_oneshot_request<T>(&self, req: RpcRequest) -> Option<T>
    where
        T: 'static + Serialize,
    {
        let (tx, rx) = oneshot::channel::<T>();
        let responder = Box::new(tx);
        let mut sender = self.rpc_sender.clone();
        sender.send(WasmRpcRequest { req, responder }).await;

        rx.await.ok()
    }

    pub async fn pubsub_publish(&self, topic: PubsubTopic, msg: GossipNetMessageV1) -> JsValue {
        let req = RpcRequest::P2pPubsubPublish(topic, msg);
        let res = self
            .rpc_oneshot_request::<RpcP2pPubsubPublishResponse>(req)
            .await;
        JsValue::from_serde(&res).unwrap()
    }
}

#[wasm_bindgen]
impl JsHandle {
    pub fn is_peer_id_valid(&self, id: &str) -> Result<(), String> {
        id.parse::<lib::p2p::PeerId>()
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn global_state_get(&self) -> JsValue {
        let req = RpcRequest::GetState;
        let res = self.rpc_oneshot_request::<RpcStateGetResponse>(req).await;
        JsValue::from_serde(&res).unwrap()
    }

    pub async fn peer_connect(&self, addr: String) -> Result<String, JsValue> {
        let addr = Multiaddr::from_str(&addr).map_err(|err| err.to_string())?;
        let peer_id =
            peer_id_from_addr(&addr).ok_or_else(|| "Multiaddr missing PeerId".to_string())?;

        let req = RpcRequest::P2pConnectionOutgoing(P2pConnectionOutgoingInitOpts {
            peer_id,
            addrs: vec![addr],
        });
        self.rpc_oneshot_request::<RpcP2pConnectionOutgoingResponse>(req)
            .await
            .ok_or_else(|| JsValue::from("state machine shut down"))??;

        Ok(peer_id.to_string())
    }

    #[wasm_bindgen(js_name = pubsub_publish)]
    pub async fn js_pubsub_publish(&self, topic: String, msg: JsValue) -> Result<(), JsValue> {
        let topic = PubsubTopic::from_str(&topic).map_err(|err| err.to_string())?;
        let msg = msg.into_serde().map_err(|err| err.to_string())?;
        let req = RpcRequest::P2pPubsubPublish(topic, msg);
        self.rpc_oneshot_request::<RpcP2pPubsubPublishResponse>(req)
            .await;
        Ok(())
    }

    pub fn generate_account_keys(&self) -> JsValue {
        let mut r = rand::rngs::OsRng::default();
        let keypair = mina_signer::Keypair::rand(&mut r);
        let priv_key = keypair.to_hex();
        let pub_key = keypair.get_address();
        JsValue::from_serde(&serde_json::json!({
            "priv_key": priv_key,
            "pub_key": pub_key,
        }))
        .unwrap()
    }

    pub async fn payment_sign_and_inject(&mut self, data: JsValue) -> Result<JsValue, JsValue> {
        #[serde_as]
        #[derive(serde::Deserialize)]
        struct Payment {
            priv_key: String,
            to: String,
            #[serde_as(as = "DisplayFromStr")]
            amount: u64,
            #[serde_as(as = "DisplayFromStr")]
            fee: u64,
            #[serde_as(as = "DisplayFromStr")]
            nonce: u32,
            memo: Option<String>,
        }

        let data: Payment = data
            .into_serde()
            .map_err(|err| format!("Deserialize Error: {}", err))?;
        let keypair = keypair_from_bs58check_secret_key(&data.priv_key)?;
        let to =
            PubKey::from_address(&data.to).map_err(|err| format!("Bad `to` address: {}", err))?;

        let mut tx = Transaction::new_payment(
            keypair.public.clone(),
            to,
            data.amount * 1_000_000_000,
            data.fee * 1_000_000_000,
            data.nonce,
        );
        if let Some(memo) = data.memo.filter(|s| !s.is_empty()) {
            tx = tx.set_memo_str(&memo);
        }
        let sig = self.signer.sign(&keypair, &tx);

        let msg = tx.to_gossipsub_v1_msg(sig);
        log::info!("created transaction pool message: {:?}", msg);
        self.pubsub_publish(PubsubTopic::CodaConsensusMessage, msg)
            .await;
        Ok(JsValue::NULL)
    }
}

fn peer_id_from_addr(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|p| match p {
        MultiaddrProtocol::P2p(id) => PeerId::from_multihash(id).ok(),
        _ => None,
    })
}
