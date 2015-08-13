use std::cell::UnsafeCell;
use std::io;
use std::os::unix::io::AsRawFd;
use std::convert::From;
use std::sync::Arc;
use std::thread;

use mio::{Evented, Handler, Sender};

use mio::util::BoundedQueue;

use scheduler::{Scheduler, CoroutineRefMut, SchedMessage};
use coroutine::{self, Coroutine, State, Handle};

thread_local!(static PROCESSOR: UnsafeCell<Processor> = UnsafeCell::new(Processor::new()));

pub struct Processor {
    event_loop_hdl: Sender<SchedMessage>,
    work_queue: Arc<BoundedQueue<CoroutineRefMut>>,
    main_coro: Handle,
    cur_running: Option<CoroutineRefMut>,
    last_result: Option<coroutine::Result<State>>,
}

impl Processor {
    pub fn new() -> Processor {
        let main_coro = unsafe {
            Coroutine::empty()
        };

        Processor {
            event_loop_hdl: Scheduler::get().get_eventloop_handle(),
            work_queue: Scheduler::get().get_queue(),
            main_coro: main_coro,
            cur_running: None,
            last_result: None,
        }
    }

    pub fn running(&mut self) -> Option<CoroutineRefMut> {
        self.cur_running
    }

    pub fn current() -> &'static mut Processor {
        PROCESSOR.with(|p| unsafe { &mut *p.get() })
    }

    pub fn set_last_result(&mut self, r: coroutine::Result<State>) {
        self.last_result = Some(r);
    }

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
                    if Scheduler::get().work_count() == 0 {
                        break;
                    } else {
                        thread::sleep_ms(100);
                    }
                }
            }
        }

        Ok(())
    }

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

    pub fn sched(&mut self) {
        match self.cur_running.take() {
            None => {},
            Some(coro_ref) => unsafe {
                self.set_last_result(Ok(State::Suspended));
                (&mut *coro_ref.coro_ptr).yield_to(&*self.main_coro)
            }
        }
    }

    pub fn block(&mut self) {
        match self.cur_running.take() {
            None => {},
            Some(coro_ref) => unsafe {
                self.set_last_result(Ok(State::Blocked));
                (&mut *coro_ref.coro_ptr).yield_to(&*self.main_coro)
            }
        }
    }

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

#[cfg(any(target_os = "macos",
          target_os = "freebsd",
          target_os = "dragonfly",
          target_os = "ios",
          target_os = "bitrig",
          target_os = "openbsd",
          target_os = "linux",
          target_os = "android"))]
impl Processor {
    pub fn read_event<E: Evented + AsRawFd>(&mut self, fd: &E) {
        Scheduler::run_loop();
        self.event_loop_hdl.send(SchedMessage::ReadEvent(From::from(fd.as_raw_fd()),
                                                         Processor::current().running().unwrap())).unwrap();

        Scheduler::block();
    }

    pub fn write_event<E: Evented + AsRawFd>(&mut self, fd: &E) {
        Scheduler::run_loop();
        self.event_loop_hdl.send(SchedMessage::WriteEvent(From::from(fd.as_raw_fd()),
                                                         Processor::current().running().unwrap())).unwrap();

        Scheduler::block();
    }
}
