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

use coroutine::{State, Handle, Coroutine};

use mio::{EventLoop, Evented, Handler, Token, ReadHint, Interest, PollOpt};
use mio::util::Slab;
#[cfg(target_os = "linux")]
use mio::Io;

use mio::util::BoundedQueue;

use scheduler::Scheduler;

thread_local!(static PROCESSOR: UnsafeCell<Processor> = UnsafeCell::new(Processor::new()));

pub struct Processor {
    event_loop: EventLoop<IoHandler>,
    work_queue: Arc<BoundedQueue<Handle>>,
    handler: IoHandler,
}

impl Processor {
    pub fn new() -> Processor {
        Processor {
            event_loop: EventLoop::new().unwrap(),
            work_queue: Scheduler::get().get_queue(),
            handler: IoHandler::new(),
        }
    }

    pub fn current() -> &'static mut Processor {
        PROCESSOR.with(|p| unsafe { &mut *p.get() })
    }

    pub fn schedule(&mut self) -> io::Result<()> {
        loop {
            if self.handler.slabs.count() != 0 {
                try!(self.event_loop.run_once(&mut self.handler));
            }

            match self.work_queue.pop() {
                Some(hdl) => {
                    match hdl.resume() {
                        Ok(State::Suspended) => {
                            Scheduler::ready(hdl);
                        },
                        Ok(State::Finished) | Ok(State::Panicked) => {
                            Scheduler::finished(hdl);
                        },
                        Ok(State::Blocked) => (),
                        Ok(..) => unreachable!(),
                        Err(err) => {
                            error!("Coroutine resume failed, {:?}", err);
                            Scheduler::finished(hdl);
                        }
                    }
                },
                None => {
                    if Scheduler::get().work_count() == 0 {
                        break;
                    }
                    if self.handler.slabs.count() == 0 {
                        thread::sleep_ms(100);
                    }
                }
            }
        }

        Ok(())
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
    pub fn wait_event<E: Evented + AsRawFd>(&mut self, fd: &E, interest: Interest) -> io::Result<()> {
        let token = self.handler.slabs.insert((Coroutine::current().clone(), From::from(fd.as_raw_fd()))).unwrap();
        try!(self.event_loop.register_opt(fd, token, interest,
                                         PollOpt::level()|PollOpt::oneshot()));

        debug!("wait_event: Blocked current Coroutine ...; token={:?}", token);
        Coroutine::block();
        debug!("wait_event: Waked up; token={:?}", token);

        Ok(())
    }
}

#[cfg(any(target_os = "linux",
          target_os = "android"))]
struct IoHandler {
    slabs: Slab<(Handle, Io)>,
}

#[cfg(any(target_os = "linux",
          target_os = "android"))]
impl Handler for IoHandler {
    type Timeout = ();
    type Message = ();

    fn writable(&mut self, event_loop: &mut EventLoop<Self>, token: Token) {

        debug!("In writable, token {:?}", token);

        match self.slabs.remove(token) {
            Some((hdl, fd)) => {
                // Linux EPoll needs to explicit EPOLL_CTL_DEL the fd
                event_loop.deregister(&fd).unwrap();
                mem::forget(fd);
                Scheduler::current().ready(hdl);
            },
            None => {
                warn!("No coroutine is waiting on writable {:?}", token);
            }
        }

    }

    fn readable(&mut self, event_loop: &mut EventLoop<Self>, token: Token, hint: ReadHint) {

        debug!("In readable, token {:?}, hint {:?}", token, hint);

        match self.slabs.remove(token) {
            Some((hdl, fd)) => {
                // Linux EPoll needs to explicit EPOLL_CTL_DEL the fd
                event_loop.deregister(&fd).unwrap();
                mem::forget(fd);
                Scheduler::current().ready(hdl);
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
    pub fn wait_event<E: Evented>(&mut self, fd: &E, interest: Interest) -> io::Result<()> {
        let token = self.handler.slabs.insert(Coroutine::current().clone()).unwrap();
        try!(self.event_loop.register_opt(fd, token, interest,
                                         PollOpt::level()|PollOpt::oneshot()));

        debug!("wait_event: Blocked current Coroutine ...; token={:?}", token);
        Coroutine::block();
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
    slabs: Slab<Handle>,
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

    fn writable(&mut self, _: &mut EventLoop<Self>, token: Token) {

        debug!("In writable, token {:?}", token);

        match self.slabs.remove(token) {
            Some(hdl) => {
                Scheduler::ready(hdl);
            },
            None => {
                warn!("No coroutine is waiting on writable {:?}", token);
            }
        }

    }

    fn readable(&mut self, _: &mut EventLoop<Self>, token: Token, hint: ReadHint) {

        debug!("In readable, token {:?}, hint {:?}", token, hint);

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
