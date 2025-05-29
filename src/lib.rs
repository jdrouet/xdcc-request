#![doc = include_str!("../readme.md")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::Stream;
use irc::client::data::Config;
use irc::client::{Client, ClientStream};
use irc::error::{Error, Result};
use irc::proto::Message;
use names::Generator;

mod response;

pub use response::Response;

/// Internal engine state, shared across requests.
struct InnerEngine {
    /// Name generator for IRC nicknames.
    names: Mutex<Generator<'static>>,
    /// Timeout duration for IRC responses.
    timeout: Duration,
    /// Number of times the file will be requested
    retry_request: u8,
}

impl Default for InnerEngine {
    fn default() -> Self {
        Self {
            names: Default::default(),
            timeout: Duration::from_secs(30),
            retry_request: 5,
        }
    }
}

impl std::fmt::Debug for InnerEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(InnerEngine))
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl InnerEngine {
    /// Generate the next unique IRC nickname.
    fn next_name(&self) -> Option<String> {
        if let Ok(mut lock) = self.names.lock() {
            lock.next()
        } else {
            None
        }
    }
}

/// A clonable interface to create and manage IRC XDCC requests.
#[derive(Clone, Debug, Default)]
pub struct Engine(Arc<InnerEngine>);

impl Engine {
    /// Create a new XDCC `Request` using the given parameters.
    ///
    /// # Arguments
    ///
    /// * `server` - IRC server address.
    /// * `channel` - IRC channel to join.
    /// * `botname` - Bot's nickname to send the XDCC request to.
    /// * `packnum` - XDCC pack number.
    pub fn create_request(
        &self,
        server: impl Into<String>,
        channel: impl Into<String>,
        botname: impl Into<String>,
        packnum: u64,
    ) -> Request {
        Request {
            inner: self.0.clone(),
            info: RequestInfo {
                server: server.into(),
                channel: channel.into(),
                botname: botname.into(),
                packnum,
            },
        }
    }
}

/// Information needed to perform a XDCC request.
#[derive(Clone, Debug)]
pub struct RequestInfo {
    /// IRC server address.
    pub server: String,
    /// IRC channel to join.
    pub channel: String,
    /// Bot nickname to send request to.
    pub botname: String,
    /// XDCC pack number.
    pub packnum: u64,
}

/// A single XDCC request created from an `Engine`.
#[derive(Debug)]
pub struct Request {
    inner: Arc<InnerEngine>,
    info: RequestInfo,
}

/// Waits for the first private message from the IRC server.
///
/// Returns `Ok(())` if a `PRIVMSG` is received, or an error if the stream ends or fails.
async fn wait_for_init(
    channel: &str,
    mut stream: impl Stream<Item = Result<Message>> + Unpin,
) -> Result<()> {
    use futures_util::StreamExt;
    use irc::proto::Command;

    while let Some(message) = stream.next().await.transpose()? {
        match message.command {
            Command::JOIN(_, _, _) | Command::MOTD(_) => {
                return Ok(());
            }
            Command::PRIVMSG(origin, _) if origin == channel => {
                return Ok(());
            }
            _ => {}
        }
    }

    Err(Error::AsyncChannelClosed)
}

/// Waits for a DCC SEND response from the IRC bot.
///
/// Returns a parsed [`Response`] or an error if the stream ends or times out.
async fn wait_for_dcc_response(
    mut stream: impl Stream<Item = Result<Message>> + Unpin,
) -> Result<Response> {
    use futures_util::StreamExt;

    while let Some(message) = stream.next().await.transpose()? {
        let irc::proto::Command::PRIVMSG(_, cmd) = message.command else {
            continue;
        };
        if let Some(res) = Response::decode(&cmd) {
            return Ok(res);
        }
    }

    Err(Error::AsyncChannelClosed)
}

impl Request {
    async fn send_request(
        &self,
        client: &mut Client,
        stream: &mut ClientStream,
    ) -> Result<Response> {
        client.send_privmsg(
            self.info.botname.as_str(),
            format!("xdcc send #{}", self.info.packnum),
        )?;

        tokio::time::timeout(self.inner.timeout, wait_for_dcc_response(stream))
            .await
            .map_err(|_| Error::PingTimeout)?
    }

    /// Executes the XDCC request by connecting to the IRC server,
    /// identifying, joining the channel, sending the XDCC command,
    /// and awaiting the DCC SEND response.
    ///
    /// # Errors
    ///
    /// Returns a [`Result`] with IRC or timeout errors.
    pub async fn execute(&self) -> Result<Response> {
        let config = Config {
            nickname: self.inner.next_name(),
            server: Some(self.info.server.clone()),
            channels: vec![self.info.channel.clone()],
            use_tls: Some(true),
            ..Default::default()
        };

        let mut client = Client::from_config(config).await?;
        client.identify()?;

        let mut stream = client.stream()?;

        tokio::time::timeout(
            self.inner.timeout,
            wait_for_init(&self.info.channel, &mut stream),
        )
        .await
        .map_err(|_| Error::PingTimeout)??;

        let mut index: u8 = 0;
        loop {
            match self.send_request(&mut client, &mut stream).await {
                Ok(res) if res.port == 0 => {
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "unsupported DCC response",
                    )));
                }
                Ok(res) => {
                    return Ok(res);
                }
                Err(err) => {
                    if index >= self.inner.retry_request {
                        return Err(err);
                    }
                    index += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use irc::proto::{Command, Message};

    #[tokio::test]
    async fn should_wait_for_dcc_message() {
        let mut stream = stream::iter(vec![Ok(Message {
            tags: None,
            prefix: None,
            command: Command::PRIVMSG(
                "botname".into(),
                "DCC SEND \"ubuntu.iso\" 3232235777 5000 1048576".into(),
            ),
        })]);
        let res = super::wait_for_dcc_response(&mut stream).await.unwrap();
        assert_eq!(res.filename, "ubuntu.iso");
    }

    #[tokio::test]
    async fn should_wait_for_private_message() {
        let mut stream = stream::iter(vec![
            Ok(Message {
                tags: None,
                prefix: None,
                command: Command::PING(Default::default(), Default::default()),
            }),
            Ok(Message {
                tags: None,
                prefix: None,
                command: Command::PRIVMSG("botname".into(), "hello world".into()),
            }),
        ]);
        super::wait_for_init("botname", &mut stream).await.unwrap();
    }

    #[tokio::test]
    async fn should_fail_if_no_private_message() {
        let mut stream = stream::iter(vec![Ok(Message {
            tags: None,
            prefix: None,
            command: Command::PING(Default::default(), Default::default()),
        })]);
        super::wait_for_init("", &mut stream).await.unwrap_err();
    }
}
