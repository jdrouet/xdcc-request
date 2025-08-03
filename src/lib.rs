#![doc = include_str!("../readme.md")]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::Stream;
use irc::client::Client;
use irc::client::data::Config;
use irc::error::{Error, Result};
use irc::proto::Message;
use names::Generator;

/// Internal engine state, shared across requests.
struct InnerEngine {
    /// Name generator for IRC nicknames.
    nicknames: Mutex<Generator<'static>>,
    /// Timeout duration for IRC responses.
    timeout: Duration,
    /// Username generator for IRC usernames.
    usernames: Option<Mutex<Generator<'static>>>,
}

impl Default for InnerEngine {
    fn default() -> Self {
        Self {
            nicknames: Default::default(),
            timeout: Duration::from_secs(30),
            usernames: Default::default(),
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
    fn next_nickname(&self) -> Option<String> {
        if let Ok(mut lock) = self.nicknames.lock() {
            lock.next()
        } else {
            None
        }
    }

    /// Generate the next unique IRC username.
    fn next_username(&self) -> Option<String> {
        if let Some(usernames) = &self.usernames {
            if let Ok(mut lock) = usernames.lock() {
                return lock.next();
            }
        }
        None
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
async fn wait_for_first_private_message(
    mut stream: impl Stream<Item = Result<Message>> + Unpin,
) -> Result<()> {
    use futures_util::StreamExt;

    while let Some(message) = stream.next().await.transpose()? {
        if matches!(message.command, irc::proto::Command::PRIVMSG(_, _)) {
            return Ok(());
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
        let irc::proto::Command::PRIVMSG(_botname, cmd) = message.command else {
            continue;
        };
        if let Some(res) = Response::decode(&cmd) {
            return Ok(res);
        }
    }

    Err(Error::AsyncChannelClosed)
}

impl Request {
    /// Executes the XDCC request by connecting to the IRC server,
    /// identifying, joining the channel, sending the XDCC command,
    /// and awaiting the DCC SEND response.
    ///
    /// # Errors
    ///
    /// Returns a [`Result`] with IRC or timeout errors.
    pub async fn execute(&self) -> Result<Response> {
        let config = Config {
            nickname: self.inner.next_nickname(),
            username: self.inner.next_username(),
            server: Some(self.info.server.clone()),
            channels: vec![self.info.channel.clone()],
            ..Default::default()
        };

        let mut client = Client::from_config(config).await?;
        client.identify()?;

        let mut stream = client.stream()?;
        tokio::time::timeout(
            self.inner.timeout,
            wait_for_first_private_message(&mut stream),
        )
        .await
        .map_err(|_| Error::PingTimeout)??;

        client.send_privmsg(
            self.info.botname.as_str(),
            format!("xdcc send #{}", self.info.packnum),
        )?;

        tokio::time::timeout(self.inner.timeout, wait_for_dcc_response(&mut stream))
            .await
            .map_err(|_| Error::PingTimeout)?
    }
}

/// Represents a parsed DCC SEND response from the IRC bot.
#[derive(Clone, Debug)]
pub struct Response {
    /// The name of the file being sent.
    pub filename: String,
    /// IP address of the sender.
    pub address: IpAddr,
    /// Port number used for the DCC transfer.
    pub port: u16,
    /// Size of the file in bytes.
    pub filesize: u64,
}

impl Response {
    /// Decodes a `DCC SEND` command message into a `Response`.
    ///
    /// Returns `Some(Response)` if decoding is successful, or `None` if parsing fails.
    pub fn decode(msg: &str) -> Option<Self> {
        let msg = msg.trim().strip_prefix("DCC SEND ")?;

        let (msg, filesize) = msg.rsplit_once(" ")?;
        let filesize = filesize.parse::<u64>().ok()?;

        let (msg, port) = msg.rsplit_once(" ")?;
        let port = port.parse::<u16>().ok()?;

        let (msg, ip) = msg.rsplit_once(" ")?;
        let ip = ip.parse::<u32>().ok()?;
        let ip = Ipv4Addr::from(ip);

        let filename = msg.trim_matches('"');
        let filename = filename.replace("\\\"", "\"");

        Some(Self {
            filename,
            address: IpAddr::V4(ip),
            port,
            filesize,
        })
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
        super::wait_for_first_private_message(&mut stream)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn should_fail_if_no_private_message() {
        let mut stream = stream::iter(vec![Ok(Message {
            tags: None,
            prefix: None,
            command: Command::PING(Default::default(), Default::default()),
        })]);
        super::wait_for_first_private_message(&mut stream)
            .await
            .unwrap_err();
    }

    #[test_case::test_case("DCC SEND \"foo.txt\" 3232235777 5000 1048576", "foo.txt", 5000, 1048576; "simple")]
    #[test_case::test_case("DCC SEND \"hello\\\"world.txt\" 3232235777 5000 1048576", "hello\"world.txt", 5000, 1048576; "with quotes")]
    #[test_case::test_case("DCC SEND \"foo bar baz.txt\" 3232235777 5000 1048576", "foo bar baz.txt", 5000, 1048576; "filename with spaces")]
    fn should_decode_dcc_msg(msg: &str, fname: &str, port: u16, size: u64) {
        let res = super::Response::decode(msg).unwrap();
        assert_eq!(res.filename, fname);
        assert_eq!(res.port, port);
        assert_eq!(res.filesize, size);
    }
}
