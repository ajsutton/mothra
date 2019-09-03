use crate::discovery::Discovery;
use crate::rpc::{RPCEvent, RPCMessage, RPC};
use crate::{error, NetworkConfig};
use crate::{Topic, TopicHash};
use futures::prelude::*;
use libp2p::{
    core::identity::Keypair,
    discv5::Discv5Event,
    gossipsub::{Gossipsub, GossipsubEvent},
    identify::{Identify, IdentifyEvent},
    ping::{Ping, PingConfig, PingEvent},
    swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess},
    tokio_io::{AsyncRead, AsyncWrite},
    NetworkBehaviour, PeerId,
};
use slog::{o, debug};
use std::num::NonZeroU32;
use std::time::Duration;
use std::collections::HashSet;

const MAX_IDENTIFY_ADDRESSES: usize = 20;

/// Builds the network behaviour that manages the core protocols of eth2.
/// This core behaviour is managed by `Behaviour` which adds peer management to all core
/// behaviours.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "BehaviourEvent", poll_method = "poll")]
pub struct Behaviour<TSubstream: AsyncRead + AsyncWrite> {
    /// The routing pub-sub mechanism for eth2.
    gossipsub: Gossipsub<TSubstream>,
    /// The serenity RPC specified in the wire-0 protocol.
    serenity_rpc: RPC<TSubstream>,
    /// Keep regular connection to peers and disconnect if absent.
    ping: Ping<TSubstream>,
    // TODO: Using id for initial interop. This will be removed by mainnet.
    /// Provides IP addresses and peer information.
    identify: Identify<TSubstream>,
    /// Kademlia for peer discovery.
    discovery: Discovery<TSubstream>,
    #[behaviour(ignore)]
    /// The events generated by this behaviour to be consumed in the swarm poll.
    events: Vec<BehaviourEvent>,
    /// Logger for behaviour actions.
    #[behaviour(ignore)]
    log: slog::Logger,
}

impl<TSubstream: AsyncRead + AsyncWrite> Behaviour<TSubstream> {
    pub fn new(
        local_key: &Keypair,
        net_conf: &NetworkConfig,
        log: &slog::Logger,
    ) -> error::Result<Self> {
        let local_peer_id = local_key.public().clone().into_peer_id();
        let behaviour_log = log.new(o!());
        let ping_config = PingConfig::new()
            .with_timeout(Duration::from_secs(30))
            .with_interval(Duration::from_secs(20))
            .with_max_failures(NonZeroU32::new(2).expect("2 != 0"))
            .with_keep_alive(false);

        let identify = Identify::new(
            "mothra/libp2p".into(),
            "0.0.1".to_string(),
            local_key.public(),
        );


        Ok(Behaviour {
            serenity_rpc: RPC::new(log),
            gossipsub: Gossipsub::new(local_peer_id.clone(), net_conf.gs_config.clone()),
            discovery: Discovery::new(local_key, net_conf, log)?,
            ping: Ping::new(ping_config),
            identify,
            events: Vec::new(),
            log: behaviour_log,
        })
    }
}

// Implement the NetworkBehaviourEventProcess trait so that we can derive NetworkBehaviour for Behaviour
impl<TSubstream: AsyncRead + AsyncWrite> NetworkBehaviourEventProcess<GossipsubEvent>
    for Behaviour<TSubstream>
{
    fn inject_event(&mut self, event: GossipsubEvent) {
        match event {
            GossipsubEvent::Message(gs_msg) => {
                //debug!(self.log, "Received GossipEvent"; "msg" => format!("{:?}", gs_msg));

                //let msg = PubsubMessage::from_topics(&gs_msg.topics, gs_msg.data);

                self.events.push(BehaviourEvent::PubsubMessage {
                    source: gs_msg.source,
                    topics: gs_msg.topics,
                    message: gs_msg.data
                });
            }
            GossipsubEvent::Subscribed { .. } => {}
            GossipsubEvent::Unsubscribed { .. } => {}
        }
    }
}

impl<TSubstream: AsyncRead + AsyncWrite> NetworkBehaviourEventProcess<RPCMessage>
    for Behaviour<TSubstream>
{
    fn inject_event(&mut self, event: RPCMessage) {
        match event {
            RPCMessage::PeerDialed(peer_id) => {
                self.events.push(BehaviourEvent::PeerDialed(peer_id))
            }
            RPCMessage::PeerDisconnected(peer_id) => {
                self.events.push(BehaviourEvent::PeerDisconnected(peer_id))
            }
            RPCMessage::RPC(peer_id, rpc_event) => {
                self.events.push(BehaviourEvent::RPC(peer_id, rpc_event))
            }
        }
    }
}

impl<TSubstream: AsyncRead + AsyncWrite> NetworkBehaviourEventProcess<PingEvent>
    for Behaviour<TSubstream>
{
    fn inject_event(&mut self, _event: PingEvent) {
        // not interested in ping responses at the moment.
    }
}

impl<TSubstream: AsyncRead + AsyncWrite> Behaviour<TSubstream> {
    /// Consumes the events list when polled.
    fn poll<TBehaviourIn>(
        &mut self,
    ) -> Async<NetworkBehaviourAction<TBehaviourIn, BehaviourEvent>> {
        if !self.events.is_empty() {
            return Async::Ready(NetworkBehaviourAction::GenerateEvent(self.events.remove(0)));
        }

        Async::NotReady
    }
}

impl<TSubstream: AsyncRead + AsyncWrite> NetworkBehaviourEventProcess<IdentifyEvent>
    for Behaviour<TSubstream>
{
    fn inject_event(&mut self, event: IdentifyEvent) {
        match event {
            IdentifyEvent::Identified {
                peer_id, mut info, ..
            } => {
                if info.listen_addrs.len() > MAX_IDENTIFY_ADDRESSES {
                    debug!(
                        self.log,
                        "More than 20 addresses have been identified, truncating"
                    );
                    info.listen_addrs.truncate(MAX_IDENTIFY_ADDRESSES);
                }
                debug!(self.log, "Identified Peer"; "Peer" => format!("{}", peer_id),
                "Protocol Version" => info.protocol_version,
                "Agent Version" => info.agent_version,
                "Listening Addresses" => format!("{:?}", info.listen_addrs),
                "Protocols" => format!("{:?}", info.protocols)
                );
            }
            IdentifyEvent::Error { .. } => {}
            IdentifyEvent::SendBack { .. } => {}
        }
    }
}

impl<TSubstream: AsyncRead + AsyncWrite> NetworkBehaviourEventProcess<Discv5Event>
    for Behaviour<TSubstream>
{
    fn inject_event(&mut self, _event: Discv5Event) {
        // discv5 has no events to inject
    }
}

/// Implements the combined behaviour for the libp2p service.
impl<TSubstream: AsyncRead + AsyncWrite> Behaviour<TSubstream> {
    /* Pubsub behaviour functions */

    /// Subscribes to a gossipsub topic.
    pub fn subscribe(&mut self, topic: Topic) -> bool {
        self.gossipsub.subscribe(topic)
    }

    /// Publishes a message on the pubsub (gossipsub) behaviour.
    pub fn publish(&mut self, topics: Vec<Topic>, message: Vec<u8>) {
        for topic in topics {
            self.gossipsub.publish(topic, message.clone());
        }
    }

    /* Eth2 RPC behaviour functions */

    /// Sends an RPC Request/Response via the RPC protocol.
    pub fn send_rpc(&mut self, peer_id: PeerId, rpc_event: RPCEvent) {
        self.serenity_rpc.send_rpc(peer_id, rpc_event);
    }

    /* Discovery / Peer management functions */
    pub fn connected_peers(&self) -> HashSet<PeerId> {
        self.discovery.connected_peers()
    }

    pub fn num_connected_peers(&self) -> usize {
        self.discovery.connected_peers().len()
    }
}

/// The types of events than can be obtained from polling the behaviour.
pub enum BehaviourEvent {
    RPC(PeerId, RPCEvent),
    PeerDialed(PeerId),
    PeerDisconnected(PeerId),
    PubsubMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
}

/// Messages that are passed to and from the pubsub (Gossipsub) behaviour.
#[derive(Debug, Clone, PartialEq)]
pub enum PubsubMessage {
    /// Gossipsub message providing notification of a new block.
    Block(Vec<u8>),
    /// Gossipsub message providing notification of a new attestation.
    Attestation(Vec<u8>),
    /// Gossipsub message from an unknown topic.
    Unknown(Vec<u8>),
}