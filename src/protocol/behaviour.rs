use libp2p::{
    floodsub::{Floodsub, FloodsubEvent},
    identity,
    mdns::{Mdns, MdnsEvent},
    swarm::{NetworkBehaviourEventProcess},
    PeerId, NetworkBehaviour,
};
use log::{info};
use once_cell::sync::Lazy;
use tokio::{sync::mpsc};

use super::common::{ListRequest, ListResponse, ListMode};
use super::helper;

static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));

#[derive(NetworkBehaviour)]
pub struct LogBehaviour
{
    pub floodsub: Floodsub,
    pub mdns: Mdns,
    #[behaviour(ignore)]
    pub response_sender: mpsc::UnboundedSender<ListResponse>,
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for LogBehaviour
{
    fn inject_event(&mut self, event: FloodsubEvent)
    {
        match event
        {
            FloodsubEvent::Message(message) =>
            {
                if let Ok(response) = serde_json::from_slice::<ListResponse>(&message.data)
                {
                    if response.receiver == PEER_ID.to_string()
                    {
                        info!("Response From {}:", message.source);
                        response.data.iter().for_each(|log| info!("{:?}", log));
                    }
                    return;
                }
                if let Ok(request) = serde_json::from_slice::<ListRequest>(&message.data)
                {
                    match request.mode
                    {
                        ListMode::All =>
                        {
                            info!(
                                "Received All Request: {:?} From {:?}",
                                request, message.source
                            );
                            helper::respond_with_logs(
                                self.response_sender.clone(),
                                message.source.to_string(),
                            );
                        }
                        ListMode::One(ref peer_id) =>
                        {
                            if peer_id == &PEER_ID.to_string()
                            {
                                info!("Received Request: {:?} From {:?}", request, message.source);
                                helper::respond_with_logs(
                                    self.response_sender.clone(),
                                    message.source.to_string(),
                                );
                            }
                        }
                    }
                    return;
                }
            }
            _ => (),
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for LogBehaviour
{
    fn inject_event(&mut self, event: MdnsEvent)
    {
        match event
        {
            MdnsEvent::Discovered(discovered_list) =>
            {
                for (peer, _addr) in discovered_list
                {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) =>
            {
                for (peer, _addr) in expired_list
                {
                    if !self.mdns.has_node(&peer)
                    {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}

pub fn get_peed_id() -> PeerId
{
    PEER_ID.clone()
}

pub fn get_keys_ref() -> &'static identity::Keypair
{
    &KEYS
}