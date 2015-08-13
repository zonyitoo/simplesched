// The MIT License (MIT)

// Copyright (c) 2015 Y. T. Chung <zonyitoo@gmail.com>

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

//! Processing unit of a thread

use std::cell::UnsafeCell;
use std::io;
#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;
#[cfg(target_os = "linux")]
use std::convert::From;
use std::sync::Arc;
use std::thread;
#[cfg(target_os = "linux")]
use std::mem;

use mio::{EventLoop, Evented, Handler, Token, EventSet, PollOpt};
use mio::util::Slab;
#[cfg(target_os = "linux")]
use mio::Io;

use mio::util::BoundedQueue;

use scheduler::{Scheduler, CoroutineRefMut};
use coroutine::{self, Coroutine, State, Handle};

thread_local!(static PROCESSOR: UnsafeCell<Processor> = UnsafeCell::new(Processor::new()));

/// Processing unit of a thread
pub struct Processor {
    event_loop: EventLoop<IoHandler>,
    work_queue: Arc<BoundedQueue<CoroutineRefMut>>,
    handler: IoHandler,
    main_coro: Handle,
    cur_running: Option<CoroutineRefMut>,
    last_result: Option<coroutine::Result<State>>,
}

impl Processor {
    #[doc(hidden)]
    pub fn new() -> Processor {
        let main_coro = unsafe {
            Coroutine::empty()
        };

        Processor {
            event_loop: EventLoop::new().unwrap(),
            work_queue: Scheduler::get().get_queue(),
            handler: IoHandler::new(),
            main_coro: main_coro,
            cur_running: None,
            last_result: None,
        }
    }

    #[doc(hidden)]
    pub fn running(&mut self) -> Option<CoroutineRefMut> {
        self.cur_running
    }

    /// Get the thread local processor
    pub fn current() -> &'static mut Processor {
        PROCESSOR.with(|p| unsafe { &mut *p.get() })
    }

    #[doc(hidden)]
    pub fn set_last_result(&mut self, r: coroutine::Result<State>) {
        self.last_result = Some(r);
    }

    #[doc(hidden)]
    pub fn schedule(&mut self) -> io::Result<()> {
        loop {
            match self.work_queue.pop() {
                Some(hdl) => {
                    match self.resume(hdl) {
                        Ok(State::Suspended) => {
                            Scheduler::ready(hdl);
                        },
                        Ok(State::Finished) | Ok(State::Panicked) => {
                            Scheduler::finished(hdl);
                        },
                        Ok(State::Blocked) => (),
                        Err(err) => {
                            error!("Coroutine resume failed, {:?}", err);
                            Scheduler::finished(hdl);
                        }
                    }
                },
                None => {
                    if self.handler.slabs.count() != 0 {
                        try!(self.event_loop.run_once(&mut self.handler));
                    } else if Scheduler::get().work_count() == 0 {
                        break;
                    } else {
                        thread::sleep_ms(100);
                    }
                }
            }
        }

        Ok(())
    }

    #[doc(hidden)]
    pub fn resume(&mut self, coro_ref: CoroutineRefMut) -> coroutine::Result<State> {
        self.cur_running = Some(coro_ref);
        unsafe {
            self.main_coro.yield_to(&mut *coro_ref.coro_ptr);
        }

        match self.last_result.take() {
            None => Ok(State::Suspended),
            Some(r) => r,
        }
    }

    /// Suspended the current running coroutine, equivalent to `Scheduler::sched`
    pub fn sched(&mut self) {
        match self.cur_running.take() {
            None => {},
            Some(coro_ref) => unsafe {
                self.set_last_result(Ok(State::Suspended));
                (&mut *coro_ref.coro_ptr).yield_to(&*self.main_coro)
            }
        }
    }

    /// Block the current running coroutine, equivalent to `Scheduler::block`
    pub fn block(&mut self) {
        match self.cur_running.take() {
            None => {},
            Some(coro_ref) => unsafe {
                self.set_last_result(Ok(State::Blocked));
                (&mut *coro_ref.coro_ptr).yield_to(&*self.main_coro)
            }
        }
    }

    /// Yield the current running coroutine with specified result
    pub fn yield_with(&mut self, r: coroutine::Result<State>) {
        match self.cur_running.take() {
            None => {},
            Some(coro_ref) => unsafe {
                self.set_last_result(r);
                (&mut *coro_ref.coro_ptr).yield_to(&*self.main_coro)
            }
        }
    }
}

const MAX_TOKEN_NUM: usize = 102400;
impl IoHandler {
    fn new() -> IoHandler {
        IoHandler {
            slabs: Slab::new(MAX_TOKEN_NUM),
        }
    }
}

#[cfg(any(target_os = "linux",
          target_os = "android"))]
impl Processor {
    /// Register and wait I/O
    pub fn wait_event<E: Evented + AsRawFd>(&mut self, fd: &E, interest: EventSet) -> io::Result<()> {
        let token = self.handler.slabs.insert((Processor::current().running().unwrap(),
                                               From::from(fd.as_raw_fd()))).unwrap();
        try!(self.event_loop.register_opt(fd, token, interest,
                                          PollOpt::edge()|PollOpt::oneshot()));

        debug!("wait_event: Blocked current Coroutine ...; token={:?}", token);
        Scheduler::block();
        debug!("wait_event: Waked up; token={:?}", token);

        Ok(())
    }
}

#[cfg(any(target_os = "linux",
          target_os = "android"))]
struct IoHandler {
    slabs: Slab<(CoroutineRefMut, Io)>,
}

#[cfg(any(target_os = "linux",
          target_os = "android"))]
impl Handler for IoHandler {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) {
        debug!("Got {:?} for {:?}", events, token);

        match self.slabs.remove(token) {
            Some((hdl, fd)) => {
                // Linux EPoll needs to explicit EPOLL_CTL_DEL the fd
                event_loop.deregister(&fd).unwrap();
                mem::forget(fd);
                Scheduler::ready(hdl);
            },
            None => {
                warn!("No coroutine is waiting on readable {:?}", token);
            }
        }
    }
}

#[cfg(any(target_os = "macos",
          target_os = "freebsd",
          target_os = "dragonfly",
          target_os = "ios",
          target_os = "bitrig",
          target_os = "openbsd"))]
impl Processor {
    /// Register and wait I/O
    pub fn wait_event<E: Evented>(&mut self, fd: &E, interest: EventSet) -> io::Result<()> {
        let token = self.handler.slabs.insert(Processor::current().running().unwrap()).unwrap();
        try!(self.event_loop.register_opt(fd, token, interest,
                                          PollOpt::edge()|PollOpt::oneshot()));

        debug!("wait_event: Blocked current Coroutine ...; token={:?}", token);
        // Coroutine::block();
        Scheduler::block();
        debug!("wait_event: Waked up; token={:?}", token);

        Ok(())
    }
}

#[cfg(any(target_os = "macos",
          target_os = "freebsd",
          target_os = "dragonfly",
          target_os = "ios",
          target_os = "bitrig",
          target_os = "openbsd"))]
struct IoHandler {
    slabs: Slab<CoroutineRefMut>,
}

#[cfg(any(target_os = "macos",
          target_os = "freebsd",
          target_os = "dragonfly",
          target_os = "ios",
          target_os = "bitrig",
          target_os = "openbsd"))]
impl Handler for IoHandler {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, _: &mut EventLoop<Self>, token: Token, events: EventSet) {
        debug!("Got {:?} for {:?}", events, token);

        match self.slabs.remove(token) {
            Some(hdl) => {
                Scheduler::ready(hdl);
            },
            None => {
                warn!("No coroutine is waiting on readable {:?}", token);
            }
        }
    }
}
