use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use tokio::task::{self, JoinHandle};
use twitch_irc::{
    login::StaticLoginCredentials,
    message::{PrivmsgMessage, ServerMessage},
    ClientConfig, SecureTCPTransport, TwitchIRCClient,
};

use crate::{
    command_handler::{twitch_api::TwitchApi, CommandHandler, CommandMessage},
    platform::{ChannelIdentifier, ExecutionContext, Permissions},
};

use super::{ChatPlatform, UserIdentifier};

#[derive(Clone)]
pub struct Twitch {
    client: Arc<RwLock<Option<TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>>>>,
    command_handler: CommandHandler,
    credentials: StaticLoginCredentials,
}

impl Twitch {
    pub fn join_channel(&self, channel: String) {
        let client = self.client.read().unwrap();
        let client = client.as_ref().unwrap();

        client.join(channel);
    }
}

#[async_trait]
impl ChatPlatform for Twitch {
    async fn init(command_handler: CommandHandler) -> Result<Box<Self>, super::ChatPlatformError> {
        let credentials = match &command_handler.twitch_api {
            Some(twitch_api) => {
                let oauth = twitch_api.get_oauth();

                let login = TwitchApi::validate_oauth(oauth).await?.login;

                tracing::info!("Logging into Twitch as {}", login);

                StaticLoginCredentials::new(login, Some(oauth.to_string()))
            }
            None => {
                tracing::info!("Twitch API not initialized! Connecting to twitch anonymously");

                StaticLoginCredentials::anonymous()
            }
        };

        Ok(Box::new(Self {
            client: Arc::new(RwLock::new(None)),
            command_handler,
            credentials,
        }))
    }

    async fn run(self) -> JoinHandle<()> {
        tracing::info!("Connected to Twitch");

        let config = ClientConfig::new_simple(self.credentials.clone());

        let (mut incoming_messages, client) =
            TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

        *self.client.write().unwrap() = Some(client.clone());

        tokio::spawn(async move {
            let command_prefix = Self::get_prefix();

            while let Some(message) = incoming_messages.recv().await {
                match message {
                    ServerMessage::Privmsg(mut pm) => {
                        tracing::debug!("{:?}", pm);

                        if let Some(message_text) = pm.message_text.strip_prefix(&command_prefix) {
                            pm.message_text = message_text.to_string();

                            let context = ExecutionContext {
                                channel: ChannelIdentifier::TwitchChannelName(pm.channel_login.clone()),
                                permissions: {
                                    if pm.badges.iter().any(|badge| badge.name == "moderator")
                                        | pm.badges.iter().any(|badge| badge.name == "broadcaster")
                                    {
                                        Permissions::ChannelMod
                                    } else {
                                        Permissions::Default
                                    }
                                },
                            };

                            let cclient = client.clone();
                            let command_handler = self.command_handler.clone();

                            task::spawn(async move {
                                let response =
                                    command_handler.handle_command_message(&pm, context, pm.get_user_identifier()).await;

                                if let Some(response) = response {
                                    tracing::info!("Replying with {}", response);

                                    cclient
                                        .reply_to_privmsg(response, &pm)
                                        .await
                                        .expect("Failed to reply");
                                }
                            });
                        }
                    }
                    // ServerMessage::Whisper(_) => {}
                    _ => (),
                }
            }
        })
    }
}

impl CommandMessage for PrivmsgMessage {
    fn get_user_identifier(&self) -> UserIdentifier {
        UserIdentifier::TwitchID(self.sender.id.clone())
    }

    fn get_text(&self) -> String {
        self.message_text.clone()
    }
}
