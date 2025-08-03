use std::net::{IpAddr, Ipv4Addr};

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
