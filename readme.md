# xdcc-request

[![Crates.io](https://img.shields.io/crates/v/xdcc-request.svg)](https://crates.io/crates/xdcc-request)
[![Docs.rs](https://docs.rs/xdcc-request/badge.svg)](https://docs.rs/xdcc-request)

Asynchronous XDCC (X Direct Client-to-Client) request library for Rust.

`xdcc-request` lets you interact with IRC bots that support XDCC to programmatically request files. It handles IRC connections, nickname generation, and DCC SEND message parsing, all in a clean, async interface.

---

## Features

- Connects to IRC servers and joins channels.
- Sends XDCC commands to bots.
- Parses and extracts DCC SEND responses (filename, IP, port, file size).
- Timeout handling and nickname generation included.

---

## Example

```toml
# Cargo.toml
[dependencies]
xdcc-request = "0.1"
tokio = { version = "1", features = ["full"] }
````

```rust
use xdcc_request::engine::Engine;

#[tokio::main]
async fn main() {
    let engine = Engine::default();

    let request = engine.create_request(
        "irc.example.net",
        "#channel",
        "BotNick",
        123, // XDCC pack number
    );

    match request.execute().await {
        Ok(response) => {
            println!("Filename: {}", response.filename);
            println!("Address: {}", response.address);
            println!("Port: {}", response.port);
            println!("Filesize: {}", response.filesize);
        }
        Err(e) => eprintln!("XDCC request failed: {:?}", e),
    }
}
```

---

## Documentation

Full API docs available at [docs.rs/xdcc-request](https://docs.rs/xdcc-request)

---

## License

* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

---

## Disclaimer

This library is for educational and legal use only. Use responsibly and only on servers and with bots where you have permission to interact.
