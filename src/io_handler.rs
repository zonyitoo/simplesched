use std::mem;

use mio::{EventSet, PollOpt, Handler, EventLoop, Token};
use mio::util::Slab;

#[cfg(any(target_os = "linux",
          target_os = "android"))]
use mio::Io;

use scheduler::{Scheduler, CoroutineRefMut, SchedMessage};

#[cfg(any(target_os = "linux",
          target_os = "android"))]
pub struct IoHandler {
    slabs: Slab<(CoroutineRefMut, Io)>,
}

#[cfg(any(target_os = "linux",
          target_os = "android"))]
impl Handler for IoHandler {
    type Timeout = ();
    type Message = SchedMessage;

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

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: SchedMessage) {
        match msg {
            SchedMessage::Exit => event_loop.shutdown(),
            SchedMessage::ReadEvent(io, coro) => {
                let token = self.slabs.insert((coro, From::from(io.as_raw_fd()))).unwrap();
                event_loop.register_opt(io, token, EventSet::readable(),
                                        PollOpt::edge()|PollOpt::oneshot()).unwrap();
                mem::forget(io);
            },
            SchedMessage::WriteEvent(io, coro) => {
                let token = self.slabs.insert(coro).unwrap();
                event_loop.register_opt(io, token, EventSet::writable(),
                                        PollOpt::edge()|PollOpt::oneshot()).unwrap();
                mem::forget(io);
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
pub struct IoHandler {
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
    type Message = SchedMessage;

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

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: SchedMessage) {
        match msg {
            SchedMessage::Exit => event_loop.shutdown(),
            SchedMessage::ReadEvent(io, coro) => {
                let token = self.slabs.insert(coro).unwrap();
                event_loop.register_opt(&io, token, EventSet::readable(),
                                        PollOpt::edge()|PollOpt::oneshot()).unwrap();
                mem::forget(io);
            },
            SchedMessage::WriteEvent(io, coro) => {
                let token = self.slabs.insert(coro).unwrap();
                event_loop.register_opt(&io, token, EventSet::writable(),
                                        PollOpt::edge()|PollOpt::oneshot()).unwrap();
                mem::forget(io);
            }
        }
    }
}

const MAX_TOKEN_NUM: usize = 102400;
impl IoHandler {
    pub fn new() -> IoHandler {
        IoHandler {
            slabs: Slab::new(MAX_TOKEN_NUM),
        }
    }
}
