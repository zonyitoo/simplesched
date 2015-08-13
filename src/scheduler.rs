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

//! Global coroutine scheduler

use std::thread;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::mem;

use mio::util::BoundedQueue;

use processor::Processor;

use coroutine::Coroutine;
use options::Options;

lazy_static! {
    static ref SCHEDULER: Scheduler = Scheduler::new();
}

#[doc(hidden)]
#[allow(raw_pointer_derive)]
#[derive(Copy, Clone, Debug)]
pub struct CoroutineRefMut {
    pub coro_ptr: *mut Coroutine,
}

impl CoroutineRefMut {
    pub fn new(coro: *mut Coroutine) -> CoroutineRefMut {
        CoroutineRefMut {
            coro_ptr: coro,
        }
    }
}

unsafe impl Send for CoroutineRefMut {}

/// Coroutine scheduler
pub struct Scheduler {
    global_queue: Arc<BoundedQueue<CoroutineRefMut>>,
    work_counts: AtomicUsize,
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

const GLOBAL_QUEUE_SIZE: usize = 0x1000;

impl Scheduler {
    fn new() -> Scheduler {
        Scheduler {
            global_queue: Arc::new(BoundedQueue::with_capacity(GLOBAL_QUEUE_SIZE)),
            work_counts: AtomicUsize::new(0),
        }
    }

    /// Get the global Scheduler
    pub fn get() -> &'static Scheduler {
        &SCHEDULER
    }

    #[doc(hidden)]
    /// A coroutine is ready for schedule
    pub fn ready(mut coro: CoroutineRefMut) {
        loop {
            match Scheduler::get().global_queue.push(coro) {
                Ok(..) => return,
                Err(h) => coro = h,
            }
        }
    }

    #[doc(hidden)]
    /// Get the global work queue
    pub fn get_queue(&self) -> Arc<BoundedQueue<CoroutineRefMut>> {
        self.global_queue.clone()
    }

    #[doc(hidden)]
    /// A coroutine is finished
    pub fn finished(coro: CoroutineRefMut) {
        Scheduler::get().work_counts.fetch_sub(1, Ordering::SeqCst);

        let boxed = unsafe { Box::from_raw(coro.coro_ptr) };
        drop(boxed);
    }

    /// Total work
    pub fn work_count(&self) -> usize {
        Scheduler::get().work_counts.load(Ordering::SeqCst)
    }

    /// Spawn a new coroutine
    pub fn spawn<F>(f: F)
        where F: FnOnce() + 'static + Send
    {
        let coro = Coroutine::spawn(f);
        Scheduler::get().work_counts.fetch_add(1, Ordering::SeqCst);

        let coro = CoroutineRefMut::new(unsafe { mem::transmute(coro) });
        Scheduler::ready(coro);
        Scheduler::sched();
    }

    /// Spawn a new coroutine with options
    pub fn spawn_opts<F>(f: F, opts: Options)
        where F: FnOnce() + 'static + Send
    {
        let coro = Coroutine::spawn_opts(f, opts);
        Scheduler::get().work_counts.fetch_add(1, Ordering::SeqCst);
        let coro = CoroutineRefMut::new(unsafe { mem::transmute(coro) });
        Scheduler::ready(coro);
        Scheduler::sched();
    }

    /// Run the scheduler with `n` threads
    pub fn run(n: usize) {
        let mut futs = Vec::new();
        for _ in 0..n {
            let fut = thread::spawn(|| {
                match Processor::current().schedule() {
                    Ok(..) => {},
                    Err(err) => panic!("Processor schedule error: {:?}", err),
                }
            });

            futs.push(fut);
        }

        for fut in futs.into_iter() {
            fut.join().unwrap();
        }
    }

    /// Suspend the current coroutine
    pub fn sched() {
        Processor::current().sched();
    }

    /// Block the current coroutine
    pub fn block() {
        Processor::current().block();
    }
}
