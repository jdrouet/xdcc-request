use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

fn decode_filename(input: &str) -> Result<(&str, &str), &str> {
    let value = input.trim_start();
    if let Some(value) = value.strip_prefix('"') {
        let mut previous_backslash = false;
        for (index, c) in value.char_indices() {
            if !previous_backslash && c == '"' {
                return Ok((&value[..index], &value[(index + 1)..]));
            }
            previous_backslash = c == '\\';
        }
        Err(input)
    } else if let Some(index) = value
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map(|(index, _)| index)
    {
        Ok(value.split_at(index))
    } else {
        Err(input)
    }
}

// only supports unsigned numbers
fn decode_number<T: FromStr>(input: &str) -> Result<(T, &str), &str> {
    let value = input.trim_start();
    if let Some(index) = value
        .char_indices()
        .find(|(_, c)| !c.is_numeric())
        .map(|(index, _)| index)
    {
        let (number, rest) = value.split_at(index);
        number
            .parse()
            .map(|number| (number, rest))
            .map_err(|_| input)
    } else {
        value
            .parse::<T>()
            .map(|number| (number, ""))
            .map_err(|_| input)
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
    /// Token when server implements it
    pub token: Option<u32>,
}

impl Response {
    /// Decodes a `DCC SEND` command message into a `Response`.
    ///
    /// Returns `Some(Response)` if decoding is successful, or `None` if parsing fails.
    pub(super) fn decode(msg: &str) -> Option<Self> {
        let msg = msg
            .trim_matches(|c: char| !c.is_ascii_alphanumeric())
            .strip_prefix("DCC SEND")?;
        let (filename, msg) = decode_filename(msg).ok()?;
        let filename = filename.replace("\\\"", "\"");

        let (ip, msg) = decode_number::<u32>(msg).ok()?;
        let ip = Ipv4Addr::from(ip);

        let (port, msg) = decode_number::<u16>(msg).ok()?;

        let (filesize, msg) = decode_number::<u64>(msg).ok()?;

        let token = decode_number::<u32>(msg).ok().map(|(token, _)| token);

        Some(Self {
            filename,
            address: IpAddr::V4(ip),
            port,
            filesize,
            token,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test_case::test_case(" \"foo bar.txt\" something", "foo bar.txt", " something"; "with double quotes")]
    #[test_case::test_case(" \"foo \\\"bar.txt\" something", "foo \\\"bar.txt", " something"; "with double quotes and backslash")]
    #[test_case::test_case(" foobar.txt something", "foobar.txt", " something"; "without double quotes")]
    fn should_decode_filename(input: &str, filename: &str, rest: &str) {
        assert_eq!(super::decode_filename(input), Ok((filename, rest)));
    }

    #[test_case::test_case("DCC SEND \"foo.txt\" 3232235777 5000 1048576", "foo.txt", 5000, 1048576; "simple")]
    #[test_case::test_case("DCC SEND \"hello\\\"world.txt\" 3232235777 5000 1048576", "hello\"world.txt", 5000, 1048576; "with quotes")]
    #[test_case::test_case("DCC SEND \"foo bar baz.txt\" 3232235777 5000 1048576", "foo bar baz.txt", 5000, 1048576; "filename with spaces")]
    #[test_case::test_case("\u{1}DCC SEND \"foo bar baz.txt\" 3232235777 5000 1048576\u{1}", "foo bar baz.txt", 5000, 1048576; "remove unicodes")]
    #[test_case::test_case("\u{1}DCC SEND Between.The.Beats.2024.BDRip.x264-HYMN.mkv 991421437 30 987794520 762\u{1}", "Between.The.Beats.2024.BDRip.x264-HYMN.mkv", 30, 987794520; "wwqwfwef")]
    fn should_decode_dcc_msg(msg: &str, fname: &str, port: u16, size: u64) {
        let res = super::Response::decode(msg).unwrap();
        assert_eq!(res.filename, fname);
        assert_eq!(res.port, port);
        assert_eq!(res.filesize, size);
    }
}
