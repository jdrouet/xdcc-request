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
