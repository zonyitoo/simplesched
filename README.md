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
    Latency     7.07ms    4.58ms  78.12ms   83.27%
    Req/Sec    29.07k     3.80k   39.00k    76.00%
  578941 requests in 10.04s, 51.90MB read
  Socket errors: connect 0, read 101, write 0, timeout 0
Requests/sec:  57667.33
Transfer/sec:      5.17MB
```

Go 1.4.2 example HTTP echo server, with `GOMAXPROCS=4`:

```
$ wrk -c 400 -t 2 http://127.0.0.1:8000/
Running 10s test @ http://127.0.0.1:8000/
  2 threads and 400 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency     6.01ms    3.68ms  96.42ms   84.52%
    Req/Sec    29.32k     6.53k   51.77k    71.21%
  583573 requests in 10.05s, 75.13MB read
  Socket errors: connect 0, read 35, write 0, timeout 0
Requests/sec:  58084.36
Transfer/sec:      7.48MB
```
