extern crate clap;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate mio;

extern crate simplesched;

use clap::{Arg, App};

use mio::buf::RingBuf;

use simplesched::Scheduler;
use simplesched::net::udp::UdpSocket;

fn main() {
    env_logger::init().unwrap();

    let matches = App::new("udp-echo")
            .version(env!("CARGO_PKG_VERSION"))
            .author("Y. T. Chung <zonyitoo@gmail.com>")
            .arg(Arg::with_name("BIND").short("b").long("bind").takes_value(true).required(true)
                    .help("Listening on this address"))
            .arg(Arg::with_name("THREADS").short("t").long("threads").takes_value(true)
                    .help("Number of threads"))
            .get_matches();

    let bind_addr = matches.value_of("BIND").unwrap().to_owned();

    Scheduler::spawn(move|| {
        let addr = bind_addr.parse().unwrap();
        let server = UdpSocket::bound(&addr).unwrap();

        info!("Listening on {:?}", server.local_addr().unwrap());

        let mut buf = RingBuf::new(1024);

        loop {
            let peer_addr = server.recv_from(&mut buf).unwrap().unwrap();
            info!("Accept connection: {:?}", peer_addr);

            server.send_to(&mut buf, &peer_addr).unwrap();
        }
    });

    Scheduler::run(matches.value_of("THREADS").unwrap_or("1").parse().unwrap());
}
