# Simple Sched

The most naive scheduler for Coroutines in Rust.

[![Build Status](https://img.shields.io/travis/zonyitoo/simplesched.svg)](https://travis-ci.org/zonyitoo/simplesched)

## Usage

```toml
[dependencies.simplesched]
git = "https://github.com/zonyitoo/simplesched.git"
```

## Basic

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

More examples could be found in `examples`.
