# Simple Sched

The most naive scheduler for Coroutines in Rust.

[![Build Status](https://img.shields.io/travis/zonyitoo/simplesched.svg)](https://travis-ci.org/zonyitoo/simplesched)
[![Crates.io](	https://img.shields.io/crates/l/simplesched.svg)](	https://crates.io/crates/simplesched)
[![Crates.io](	https://img.shields.io/crates/v/simplesched.svg)](https://crates.io/crates/simplesched)

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

## Basic Benchmark

### Test environment
* OS X 10.10.5 Beta
* MacBook Pro Late 2013
* 2.4GHz Intel Core i5
* 8 GB 1600 MHz DDR3

Release build. Run the `examples/http-echo-server.rs` with 4 threads, test it with `wrk`:

```
$ wrk -c 400 -t 2 http://127.0.0.1:8000/
Running 10s test @ http://127.0.0.1:8000/
  2 threads and 400 connections

  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency     7.62ms    5.53ms  79.79ms   83.26%
    Req/Sec    27.23k     3.78k   36.30k    73.50%
  543098 requests in 10.06s, 46.61MB read
Requests/sec:  53965.70
Transfer/sec:      4.63MB
```

Go 1.4.2 example HTTP echo server, with `GOMAXPROCS=4`:

```
$ wrk -c 400 -t 2 http://127.0.0.1:8000/
Running 10s test @ http://127.0.0.1:8000/
  2 threads and 400 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency    10.99ms   19.69ms 234.40ms   93.11%
    Req/Sec    20.22k     8.58k   47.88k    67.02%
  393495 requests in 10.08s, 50.66MB read
  Socket errors: connect 0, read 12, write 0, timeout 0
Requests/sec:  39023.62
Transfer/sec:      5.02MB
```
