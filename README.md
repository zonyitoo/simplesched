# Simple Sched

The most naive scheduler for Coroutines in Rust.

[![Build Status](https://img.shields.io/travis/zonyitoo/simplesched.svg)](https://travis-ci.org/zonyitoo/simplesched)
[![Crates.io](	https://img.shields.io/crates/l/simplesched.svg)](	https://img.shields.io/crates/l/simplesched.svg)
[![Crates.io](	https://img.shields.io/crates/v/simplesched.svg)](https://img.shields.io/crates/v/simplesched.svg)

## Usage

```toml
[dependencies]
simplesched = "*"
```

### Basic

```rust
extern crate simplesched;

use simplesched::Scheduler;

fn main() {
    Scheduler::spawn(|| {
        for _ in 0..10 {
            println!("Heil Hydra");
        }
    });

    Scheduler::run(1);
}
```

### TCP Echo Server

```rust
extern crate simplesched;

use std::io::{Read, Write};

use simplesched::net::TcpListener;
use simplesched::Scheduler;

fn main() {
    // Spawn a coroutine for accepting new connections
    Scheduler::spawn(move|| {
        let acceptor = TcpListener::bind("127.0.0.1:8080").unwrap();
        println!("Waiting for connection ...");

        for stream in acceptor.incoming() {
            let mut stream = stream.unwrap();

            println!("Got connection from {:?}", stream.peer_addr().unwrap());

            // Spawn a new coroutine to handle the connection
            Scheduler::spawn(move|| {
                let mut buf = [0; 1024];

                loop {
                    match stream.read(&mut buf) {
                        Ok(0) => {
                            println!("EOF");
                            break;
                        },
                        Ok(len) => {
                            println!("Read {} bytes, echo back", len);
                            stream.write_all(&buf[0..len]).unwrap();
                        },
                        Err(err) => {
                            println!("Error occurs: {:?}", err);
                            break;
                        }
                    }
                }

                println!("Client closed");
            });
        }
    });

    // Schedule with 4 threads
    Scheduler::run(4);
}
```

More examples could be found in `examples`.
