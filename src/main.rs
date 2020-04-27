use serenity::{
    model::
    {
        channel::Message, id::ChannelId
    },
    client,
};
use serde::{Serialize, Deserialize};
use std::error::Error;
use libzmq::{ prelude::*, RoutingId, Server };
use std::convert::TryInto;
use std::env;
use log::*;
use simplelog::*;
use std::sync::{ Mutex, Arc };
use std::collections::HashSet;
use std::thread;

#[derive(Serialize, Deserialize, Debug)]
struct RelayMessage {
    channel_id: u64,
    content: String
}

#[derive(Serialize, Deserialize, Debug)]
enum MessageWrapper {
    Message(RelayMessage),
    KeepAlive
}

impl RelayMessage {
    fn new(
        channel_id: u64,
        content: String
    ) -> RelayMessage {
        RelayMessage {
            channel_id,
            content
        }
    }
}

struct Handler {
    zmq_clients: Arc<Mutex<HashSet<RoutingId>>>,
    zmq_server: Server,
}

impl Handler {
    fn new(
        zmq_clients: Arc<Mutex<HashSet<RoutingId>>>,
        zmq_server: Server,
    ) -> Handler {
        Handler {
            zmq_clients,
            zmq_server
        }
    }
}

impl client::EventHandler for Handler {
    fn message(&self, _context: client::Context, msg: Message) {
        trace!("Got discord message");
        let mut clients = self.zmq_clients.lock().unwrap();
        let mut failing = vec![];

        let message = serde_json::to_string(&MessageWrapper::Message(RelayMessage::new(
            msg.channel_id.0,
            msg.content,
        ))).expect("Failed to serialize object");
        for client in &*clients {
            if self.zmq_server.route(&message, *client).is_err() {
                failing.push(*client);
                warn!("Failed to send zmq message");
            } else {
                trace!("Send message to ZMQ clients");
            }
        }
        for failing_client in failing {
            clients.remove(&failing_client);
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = ConfigBuilder::new()
        .add_filter_allow_str("discord_relay")
        .build();
    if TermLogger::init(LevelFilter::Info, config.clone(), TerminalMode::Mixed).is_err() {
        eprintln!("Failed to create term logger");
        if SimpleLogger::init(LevelFilter::Info, config).is_err() {
            eprintln!("Failed to create simple logger");
        }
    }

    let clients = Arc::new(Mutex::new(HashSet::new()));

    // 0mq server
    let address: libzmq::TcpAddr = "0.0.0.0:32968".try_into().expect("IP address couldn't be parsed");
    let server = libzmq::ServerBuilder::new()
        .bind(address)
        .build()
        .expect("Binding to socket failed");


    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN has to be set to a valid token");
    let mut client = client::Client::new(
        token,
        Handler::new(clients.clone(), server.clone())
    ).expect("Could not create new client.");
    let http = client.cache_and_http.http.clone();

    thread::spawn(move || {
        client.start().unwrap();
    });

    loop {
        if let Ok(msg) = server.recv_msg() {
            info!("New message");
            if let Ok(message) = serde_json::from_str::<MessageWrapper>(msg.to_str().unwrap_or("")) {
                if let Some(id) = msg.routing_id() {
                    // Add to clients
                    clients.lock().unwrap().insert(id);
                }
                if let MessageWrapper::Message(send_command) = message {
                    let channel = ChannelId(send_command.channel_id);
                    if let Ok(_) = channel.say(&http, &send_command.content) {
                    } else {
                        error!("Failed to send message to discord");
                    }
                } else {
                    trace!("Got keep alive");
                }
            } else {
                error!("Failed to parse command");
            }

        }
    }
}
