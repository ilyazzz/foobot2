use crate::{database::models::Filter, platform::ChannelIdentifier};
use connector_schema::OutgoingMessage;
use redis::{aio::MultiplexedConnection, AsyncCommands};
use regex::Regex;
use std::{collections::HashMap, fmt::Display};
use std::{
    env,
    sync::{Arc, RwLock},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct PlatformHandler {
    pub filters: Arc<RwLock<HashMap<ChannelIdentifier, Vec<Filter>>>>,
    pub redis_conn: Arc<Mutex<MultiplexedConnection>>,
}

impl PlatformHandler {
    pub async fn send_to_channel(
        &self,
        channel: ChannelIdentifier,
        mut msg: String,
    ) -> Result<(), PlatformHandlerError> {
        self.filter_message(&mut msg, &channel);

        let outgoing_channel_prefix = Arc::new(
            env::var("OUTGOING_MESSAGES_CHANNEL_PREFIX")
                .unwrap_or_else(|_| "messages.outgoing.".to_owned()),
        );

        let redis_channel = format!(
            "{outgoing_channel_prefix}{platform}",
            platform = channel
                .get_platform_name()
                .ok_or_else(|| PlatformHandlerError::Unsupported)?
        );

        let message = OutgoingMessage {
            channel_id: channel.get_channel().unwrap_or_default(),
            contents: msg,
        };

        self.redis_conn
            .lock()
            .await
            .publish(redis_channel, message)
            .await
            .map_err(|error| PlatformHandlerError::PlatformError(error.into()))
    }

    pub fn filter_message(&self, message: &mut String, channel: &ChannelIdentifier) {
        let filters = self.filters.read().expect("Failed to lock");

        tracing::trace!("Checking filters for {}", message);
        if let Some(filters) = filters.get(channel) {
            for filter in filters {
                tracing::trace!("Matching {}", filter.regex);
                match Regex::new(&filter.regex) {
                    Ok(re) => {
                        if filter.block_message {
                            if re.is_match(message) {
                                message.clear();
                                break;
                            }
                        } else {
                            let replacement = match &filter.replacement {
                                Some(replacement) => replacement,
                                None => "[Blocked]",
                            };

                            *message = re.replace_all(message, replacement).to_string();
                        }
                    }
                    Err(e) => {
                        *message = format!("failed to compile message filter regex: {}", e);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum PlatformHandlerError {
    Unsupported,
    Unconfigured,
    PlatformError(anyhow::Error),
}

impl From<anyhow::Error> for PlatformHandlerError {
    fn from(e: anyhow::Error) -> Self {
        PlatformHandlerError::PlatformError(e)
    }
}

impl std::error::Error for PlatformHandlerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl Display for PlatformHandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PlatformHandlerError::Unconfigured => String::from("Platform is not configured"),
                PlatformHandlerError::Unsupported =>
                    String::from("Remote message sending is not supported for this platform"),
                PlatformHandlerError::PlatformError(e) => format!("Platform error: {}", e),
            }
        )
    }
}
