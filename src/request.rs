use crate::inner_engine::InnerEngine;
use crate::request_info::RequestInfo;
use crate::response::Response;
use futures_util::Stream;
use irc::client::Client;
use irc::client::data::Config;
use irc::error::{Error, Result};
use irc::proto::Message;
use std::sync::Arc;

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

/// A single XDCC request created from an `Engine`.
#[derive(Debug)]
pub struct Request {
    pub inner: Arc<InnerEngine>,
    pub info: RequestInfo,
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
}
