use std::sync::Arc;

use crate::{inner_engine::InnerEngine, request::Request, request_info::RequestInfo};

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
