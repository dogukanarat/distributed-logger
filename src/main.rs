use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, Topic},
    futures::StreamExt,
    mdns::{Mdns},
    mplex,
    noise::{Keypair, NoiseConfig, X25519Spec},
    swarm::{Swarm, SwarmBuilder},
    tcp::TokioTcpConfig,
    Transport,
};
use log::{error, info};
use once_cell::sync::Lazy;
use std::collections::HashSet;
use tokio::{io::AsyncBufReadExt, sync::mpsc};

mod protocol;

use protocol::{
    common::{ListMode, ListRequest, EventType},
    behaviour::{LogBehaviour},
    helper, behaviour
};

static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("logging"));

#[tokio::main]
async fn main()
{
    pretty_env_logger::init();

    info!("Peer Id: {}", behaviour::get_peed_id());

    let (response_sender, mut response_receiver) = mpsc::unbounded_channel();

    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(behaviour::get_keys_ref())
        .expect("can create auth keys");

    let transport = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    let mut behaviour = LogBehaviour {
        floodsub: Floodsub::new(behaviour::get_peed_id()),
        mdns: Mdns::new(Default::default())
            .await
            .expect("can create mdns"),
        response_sender,
    };

    behaviour.floodsub.subscribe(TOPIC.clone());

    let mut swarm = SwarmBuilder::new(transport, behaviour, behaviour::get_peed_id())
        .executor(Box::new(|future| {
            tokio::spawn(future);
        }))
        .build();

    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("can get a local socket"),
    )
    .expect("swarm can be started");

    loop
    {
        let event = {
            tokio::select! {
                line = stdin.next_line() => 
                {
                    Some(
                        EventType::Input(
                            line.expect("can get line")
                                .expect("can read line from stdin")
                        )
                    )
                },
                response = response_receiver.recv() => 
                {
                    Some(
                        EventType::Response(
                            response.expect("response exists")
                        )
                    )
                },
                event = swarm.select_next_some() => 
                {
                    info!("Unhandled Swarm Event: {:?}", event);
                    None
                },
            }
        };

        if let Some(event) = event
        {
            match event
            {
                EventType::Response(response) =>
                {
                    let json = serde_json::to_string(&response).expect("can jsonify response");

                    swarm
                        .behaviour_mut()
                        .floodsub
                        .publish(TOPIC.clone(), json.as_bytes());
                }
                EventType::Input(line) => match line.as_str()
                {
                    cmd if cmd.starts_with("list peers") => handle_list_peers(&mut swarm).await,
                    cmd if cmd.starts_with("list logs") => handle_list_logs(cmd, &mut swarm).await,
                    cmd if cmd.starts_with("create log") => handle_create_log(cmd).await,
                    _ => error!("unknown command"),
                },
            }
        }
    }
}

async fn handle_list_peers(swarm: &mut Swarm<LogBehaviour>)
{
    info!("Discovered Peers:");

    let nodes = swarm.behaviour().mdns.discovered_nodes();

    let mut unique_peers = HashSet::new();

    for peer in nodes
    {
        unique_peers.insert(peer);
    }

    unique_peers.iter().for_each(|peer_id| info!("{}", peer_id));
}

async fn handle_list_logs(cmd: &str, swarm: &mut Swarm<LogBehaviour>)
{
    let rest = cmd.strip_prefix("list logs ");

    match rest
    {
        Some("all") =>
        {
            let request = ListRequest {
                mode: ListMode::All,
            };

            let json = serde_json::to_string(&request).expect("can jsonify request");

            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        Some(request_peer_id) =>
        {
            let request = ListRequest {
                mode: ListMode::One(request_peer_id.to_owned()),
            };

            let json = serde_json::to_string(&request).expect("can jsonify request");

            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        None =>
        {
            match helper::get_local_logs().await
            {
                Ok(vector) =>
                {
                    info!("Local Logs ({})", vector.len());
                    vector.iter().for_each(|log| info!("{:?}", log));
                }
                Err(error) => error!("error fetching local recipes: {}", error),
            };
        }
    };
}

async fn handle_create_log(cmd: &str)
{
    if let Some(content) = cmd.strip_prefix("create log -- ")
    {
        if let Err(error) = helper::create_new_log(content).await
        {
            error!("error creating log: {}", error);
        };
    }
}
