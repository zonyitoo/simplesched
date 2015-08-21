// The MIT License (MIT)

// Copyright (c) 2015 Rustcc Developers

// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! The most naive coroutine scheduler with asynchronous I/O support.
//!
//! The scheduler will hold a global lock-free queue for tasks, all worker threads
//! will take tasks from the global queue.

#![feature(libc, rt, box_raw, reflect_marker)]

extern crate context;
#[macro_use] extern crate log;
extern crate mio;
extern crate libc;
#[macro_use] extern crate lazy_static;
extern crate hyper;
extern crate url;
#[cfg(feature = "openssl")]
extern crate openssl;
extern crate bytes;

pub use scheduler::Scheduler;
pub use options::Options;

pub mod scheduler;
pub mod net;
pub mod processor;
pub mod options;
pub mod sync;
mod coroutine;

/// Spawn a new Coroutine
pub fn spawn<F>(f: F)
    where F: FnOnce() + Send + 'static
{
    Scheduler::spawn(f)
}

/// Spawn a new Coroutine with options
pub fn spawn_opts<F>(f: F, opts: Options)
    where F: FnOnce() + Send + 'static
{
    Scheduler::spawn_opts(f, opts)
}

/// Giveup the CPU
pub fn sched() {
    Scheduler::sched()
}

pub struct Builder {
    opts: Options
}

impl Builder {
    pub fn new() -> Builder {
        Builder {
            opts: Options::new()
        }
    }

    pub fn stack_size(mut self, stack_size: usize) -> Builder {
        self.opts.stack_size = stack_size;
        self
    }

    pub fn name(mut self, name: Option<String>) -> Builder {
        self.opts.name = name;
        self
    }

    pub fn spawn<F>(self, f: F)
        where F: FnOnce() + Send + 'static
    {
        Scheduler::spawn_opts(f, self.opts)
    }
}
