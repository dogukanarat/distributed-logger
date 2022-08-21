use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, FloodsubEvent, Topic},
    futures::StreamExt,
    identity,
    mdns::{Mdns, MdnsEvent},
    mplex,
    noise::{Keypair, NoiseConfig, X25519Spec},
    swarm::{NetworkBehaviourEventProcess, Swarm, SwarmBuilder},
    tcp::TokioTcpConfig,
    NetworkBehaviour, PeerId, Transport,
};
use log::{error, info};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::{fs, io::AsyncBufReadExt, sync::mpsc};
use std::time::{SystemTime};
use chrono::offset::Utc;
use chrono::DateTime;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

const STORAGE_FILE_PATH: &str = "./log-file.json";

static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("logging"));

#[derive(Debug, Serialize, Deserialize)]
struct Log {
    timestamp: String,
    content: String
}

type Logs = Vec<Log>;

#[derive(Debug, Serialize, Deserialize)]
enum ListMode {
    All,
    One(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct ListRequest {
    mode: ListMode,
}

#[derive(Debug, Serialize, Deserialize)]
struct ListResponse {
    mode: ListMode,
    data: Logs,
    receiver: String,
}

enum EventType {
    Response(ListResponse),
    Input(String),
}

#[derive(NetworkBehaviour)]
struct LogBehaviour {
    floodsub: Floodsub,
    mdns: Mdns,
    #[behaviour(ignore)]
    response_sender: mpsc::UnboundedSender<ListResponse>,
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for LogBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(message) => {
                if let Ok(response) = serde_json::from_slice::<ListResponse>(&message.data) {
                    if response.receiver == PEER_ID.to_string() {
                        info!("Response From {}:", message.source);
                        response.data.iter().for_each(|log| info!("{:?}", log));
                    }
                    return;
                } 
                if let Ok(request) = serde_json::from_slice::<ListRequest>(&message.data) {
                    match request.mode {
                        ListMode::All => {
                            info!("Received All Request: {:?} From {:?}", request, message.source);
                            respond_with_logs(
                                self.response_sender.clone(),
                                message.source.to_string(),
                            );
                        }
                        ListMode::One(ref peer_id) => {
                            if peer_id == &PEER_ID.to_string() {
                                info!("Received Request: {:?} From {:?}", request, message.source);
                                respond_with_logs(
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

impl NetworkBehaviourEventProcess<MdnsEvent> for LogBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}

fn respond_with_logs(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_local_logs().await {
            Ok(logs) => {
                let response = ListResponse {
                    mode: ListMode::All,
                    receiver,
                    data: logs.into_iter().collect(),
                };

                if let Err(error) = sender.send(response) {
                    error!("error sending response via channel, {}", error);
                }
            }
            Err(error) => {
                error!("error fetching local recipes to answer all request, {}", error)
            },
        }
    });
}

async fn create_new_log(content: &str) -> Result<()> {
    let mut local_logs = read_local_logs().await?;
    let current_timestamp : DateTime<Utc> = SystemTime::now().into();

    local_logs.push(Log {
        timestamp: format!("{}", current_timestamp.format("%d/%m/%Y %T")),
        content: content.to_owned(),
    });

    write_local_logs(&local_logs).await?;

    info!("Created Log:");
    info!("Timestamp: {}", local_logs.last().unwrap().timestamp);
    info!("Content: {}", local_logs.last().unwrap().content);

    Ok(())
}

async fn read_local_logs() -> Result<Logs> {
    let content = fs::read(STORAGE_FILE_PATH).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

async fn write_local_logs(logs: &Logs) -> Result<()> {
    let json = serde_json::to_string(&logs)?;
    fs::write(STORAGE_FILE_PATH, &json).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    info!("Peer Id: {}", PEER_ID.clone());

    let (
        response_sender,
        mut response_receiver
        ) = mpsc::unbounded_channel();

    let auth_keys = 
        Keypair::<X25519Spec>::new()
        .into_authentic(&KEYS)
        .expect("can create auth keys");

    let transport = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    let mut behaviour = LogBehaviour {
        floodsub: Floodsub::new(PEER_ID.clone()),
        mdns: Mdns::new(Default::default())
            .await
            .expect("can create mdns"),
        response_sender,
    };

    behaviour.floodsub.subscribe(TOPIC.clone());

    let mut swarm = 
        SwarmBuilder::new(
            transport,
            behaviour, 
            PEER_ID.clone()
        )
            .executor(Box::new(|future| {
                tokio::spawn(future);
            }))
            .build();

    let mut stdin = 
    tokio::io::BufReader::new(
        tokio::io::stdin()
    ).lines();

    Swarm::listen_on(
        &mut swarm,
        "/ip4/127.0.0.1/tcp/0"
            .parse()
            .expect("can get a local socket"),
    )
    .expect("swarm can be started");

    loop {
        let event = {
            tokio::select! {
                line = stdin.next_line() => {
                    Some(
                        EventType::Input(
                            line.expect("can get line")
                                .expect("can read line from stdin")
                        )
                    )
                },
                response = response_receiver.recv() => {
                    Some(EventType::Response(response.expect("response exists")))
                },
                _event = swarm.select_next_some() => {
                    // info!("Swarm Event: {:?}", event);
                    None
                },
            }
        };

        if let Some(event) = event {
            match event {
                EventType::Response(response) => {
                    let json = serde_json::to_string(&response).expect("can jsonify response");

                    swarm
                        .behaviour_mut()
                        .floodsub
                        .publish(TOPIC.clone(), json.as_bytes());

                }
                EventType::Input(line) => {
                    match line.as_str() {
                        cmd if cmd.starts_with("list peers") => handle_list_peers(&mut swarm).await,
                        cmd if cmd.starts_with("list logs") => handle_list_logs(cmd, &mut swarm).await,
                        cmd if cmd.starts_with("create log") => handle_create_log(cmd).await,
                        _ => error!("unknown command"),
                    }
                },
            }
        }
    }
}

async fn handle_list_peers(swarm: &mut Swarm<LogBehaviour>) {
    info!("Discovered Peers:");

    let nodes = swarm.behaviour().mdns.discovered_nodes();

    let mut unique_peers = HashSet::new();

    for peer in nodes {
        unique_peers.insert(peer);
    }

    unique_peers.iter().for_each(|peer_id| info!("{}", peer_id));
}

async fn handle_list_logs(cmd: &str, swarm: &mut Swarm<LogBehaviour>) {
    let rest = cmd.strip_prefix("list logs ");

    // if let Some(content) = rest {
    //     if content.is_empty() {rest = None}
    // }

    match rest {
        Some("all") => {
            let request = ListRequest {
                mode: ListMode::All,
            };
            
            let json = serde_json::to_string(&request).expect("can jsonify request");

            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        Some(request_peer_id) => {
            let request = ListRequest {
                mode: ListMode::One(request_peer_id.to_owned()),
            };

            let json = serde_json::to_string(&request).expect("can jsonify request");
            
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        None => {
            match read_local_logs().await {
                Ok(vector) => {
                    info!("Local Recipes ({})", vector.len());
                    vector.iter().for_each(|log| info!("{:?}", log));
                }
                Err(error) => error!("error fetching local recipes: {}", error),
            };
        }
    };
}

async fn handle_create_log(cmd: &str) {
    if let Some(content) = cmd.strip_prefix("create log -- ") {

        if let Err(error) = create_new_log(content).await {
            error!("error creating log: {}", error);
        };
    }
}