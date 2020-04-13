use serenity::{
    model::
    {
        channel::Message, id::ChannelId
    },
    client,
};
use serde::{Serialize, Deserialize};
use std::error::Error;
use libzmq::{prelude::*};
use std::convert::TryInto;
use std::env;
use log::*;
use simplelog::*;

#[derive(Serialize, Deserialize, Debug)]
struct SendMessage {
    channel_id: u64,
    content: String
}

struct Handler;

impl client::EventHandler for Handler {
    fn message(&self, _context: client::Context, _msg: Message) {}
}

fn main() -> Result<(), Box<dyn Error>> {
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
    )
    .expect("Failed to initialize logger")])
    .expect("Failed to initialize logger");

    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN has to be set to a valid token");
    let client = client::Client::new(token, Handler).expect("Could not create new client.");
    let http = client.cache_and_http.http.clone();

    // 0mq server
    let address: libzmq::TcpAddr = "127.0.0.1:32968".try_into().expect("IP address couldn't be parsed");
    let server = libzmq::ServerBuilder::new()
        .bind(address)
        .build()
        .expect("Binding to socket failed");

    loop {
        if let Ok(msg) = server.recv_msg() {
            info!("New message");
            if let Ok(send_command) = serde_json::from_str::<SendMessage>(msg.to_str().unwrap_or("")) {
                let channel = ChannelId(send_command.channel_id);
                if let Ok(_) = channel.say(&http, &send_command.content) {
                    if let Some(id) = msg.routing_id() {
                        if server.route("OK", id).is_err() {
                            error!("Failed to respond to 0mq client");
                        }
                    }
                } else {
                    error!("Failed to send message to discord");
                    if let Some(id) = msg.routing_id() {
                        if server.route("OK", id).is_err() {
                            error!("Failed to respond to 0mq client");
                        }
                    }
                }
            } else {
                error!("Failed to parse command");
            }

        }
    }
}
