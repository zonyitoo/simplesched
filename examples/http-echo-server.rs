extern crate clap;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate mio;
extern crate hyper;

extern crate simplesched;
extern crate openssl;

use std::net::{ToSocketAddrs, SocketAddr, Shutdown};
use std::path::Path;
use std::io::{self, Read, Write, BufWriter};
use std::sync::Arc;
use std::fmt;

use clap::{Arg, App};

use hyper::http;
use hyper::buffer::BufReader;
use hyper::server::{Request, Response, Handler};
use hyper::header::{Connection, Headers, Expect};
use hyper::version::HttpVersion;
use hyper::net::{NetworkListener, NetworkStream};
use hyper::status::StatusCode;
use hyper::uri::RequestUri::AbsolutePath;
use hyper::error::Error;
use hyper::method::Method;

use openssl::ssl::{SslStream, SslContext, SSL_VERIFY_NONE};
use openssl::ssl::SslMethod::Sslv23;
use openssl::ssl::error::StreamError as SslIoError;
use openssl::x509::X509FileType;

use simplesched::Scheduler;
use simplesched::net::tcp::{TcpStream, TcpListener};

struct Worker<'a, H: Handler + 'static>(&'a H);

impl<'a, H: Handler + 'static> Worker<'a, H> {

    fn handle_connection<S>(&self, mut stream: &mut S) where S: NetworkStream + Clone {
        debug!("Incoming stream");
        let addr = match stream.peer_addr() {
            Ok(addr) => addr,
            Err(e) => {
                error!("Peer Name error: {:?}", e);
                return;
            }
        };

        // FIXME: Use Type ascription
        let stream_clone: &mut NetworkStream = &mut stream.clone();
        let rdr = BufReader::new(stream_clone);
        let wrt = BufWriter::new(stream);

        self.keep_alive_loop(rdr, wrt, addr);
        debug!("keep_alive loop ending for {}", addr);
    }

    fn keep_alive_loop<W: Write>(&self, mut rdr: BufReader<&mut NetworkStream>, mut wrt: W, addr: SocketAddr) {
        let mut keep_alive = true;
        while keep_alive {
            let req = match Request::new(&mut rdr, addr) {
                Ok(req) => req,
                Err(Error::Io(ref e)) if e.kind() == io::ErrorKind::ConnectionAborted => {
                    trace!("tcp closed, cancelling keep-alive loop");
                    break;
                }
                Err(Error::Io(e)) => {
                    debug!("ioerror in keepalive loop = {:?}", e);
                    break;
                }
                Err(e) => {
                    //TODO: send a 400 response
                    error!("request error = {:?}", e);
                    break;
                }
            };


            if !self.handle_expect(&req, &mut wrt) {
                break;
            }

            keep_alive = http::should_keep_alive(req.version, &req.headers);
            let version = req.version;
            let mut res_headers = Headers::new();
            if !keep_alive {
                res_headers.set(Connection::close());
            }
            {
                let mut res = Response::new(&mut wrt, &mut res_headers);
                res.version = version;
                self.0.handle(req, res);
            }

            // if the request was keep-alive, we need to check that the server agrees
            // if it wasn't, then the server cannot force it to be true anyways
            if keep_alive {
                keep_alive = http::should_keep_alive(version, &res_headers);
            }

            debug!("keep_alive = {:?} for {}", keep_alive, addr);
        }

    }

    fn handle_expect<W: Write>(&self, req: &Request, wrt: &mut W) -> bool {
         if req.version == HttpVersion::Http11 && req.headers.get() == Some(&Expect::Continue) {
            let status = self.0.check_continue((&req.method, &req.uri, &req.headers));
            match write!(wrt, "{} {}\r\n\r\n", HttpVersion::Http11, status) {
                Ok(..) => (),
                Err(e) => {
                    error!("error writing 100-continue: {:?}", e);
                    return false;
                }
            }

            if status != StatusCode::Continue {
                debug!("non-100 status ({}) for Expect 100 request", status);
                return false;
            }
        }

        true
    }
}

macro_rules! try_return(
    ($e:expr) => {{
        match $e {
            Ok(v) => v,
            Err(e) => { println!("Error: {}", e); return; }
        }
    }}
);

fn echo_keepalive(mut req: Request, mut res: Response) {
    match req.uri {
        AbsolutePath(ref path) => match (&req.method, &path[..]) {
            (&Method::Get, "/") | (&Method::Get, "/echo") => {
                try_return!(res.send(b"Try POST /echo"));
                return;
            },
            (&Method::Post, "/echo") => (), // fall through, fighting mutable borrows
            _ => {
                *res.status_mut() = hyper::NotFound;
                return;
            }
        },
        _ => {
            return;
        }
    };

    let mut res = try_return!(res.start());
    try_return!(io::copy(&mut req, &mut res));
}

fn echo_close(mut req: Request, mut res: Response) {
    match req.uri {
        AbsolutePath(ref path) => match (&req.method, &path[..]) {
            (&Method::Get, "/") | (&Method::Get, "/echo") => {
                res.headers_mut().set(Connection::close());
                try_return!(res.send(b"Try POST /echo"));
                return;
            },
            (&Method::Post, "/echo") => (), // fall through, fighting mutable borrows
            _ => {
                *res.status_mut() = hyper::NotFound;
                return;
            }
        },
        _ => {
            return;
        }
    };

    let mut res = try_return!(res.start());
    try_return!(io::copy(&mut req, &mut res));
}

/// A `NetworkListener` for `HttpStream`s.
pub enum HttpListener {
    /// Http variant.
    Http(TcpListener),
    /// Https variant. The two paths point to the certificate and key PEM files, in that order.
    Https(TcpListener, Arc<SslContext>)
}

impl Clone for HttpListener {
    fn clone(&self) -> HttpListener {
        match *self {
            HttpListener::Http(ref tcp) => HttpListener::Http(tcp.try_clone().unwrap()),
            HttpListener::Https(ref tcp, ref ssl) => HttpListener::Https(tcp.try_clone().unwrap(), ssl.clone()),
        }
    }
}

impl HttpListener {

    /// Start listening to an address over HTTP.
    pub fn http<To: ToSocketAddrs>(addr: To) -> hyper::Result<HttpListener> {
        Ok(HttpListener::Http(try!(TcpListener::bind(addr))))
    }

    /// Start listening to an address over HTTPS.
    pub fn https<To: ToSocketAddrs>(addr: To, cert: &Path, key: &Path) -> hyper::Result<HttpListener> {
        let mut ssl_context = try!(SslContext::new(Sslv23));
        try!(ssl_context.set_cipher_list("DEFAULT"));
        try!(ssl_context.set_certificate_file(cert, X509FileType::PEM));
        try!(ssl_context.set_private_key_file(key, X509FileType::PEM));
        ssl_context.set_verify(SSL_VERIFY_NONE, None);
        HttpListener::https_with_context(addr, ssl_context)
    }

    /// Start listening to an address of HTTPS using the given SslContext
    pub fn https_with_context<To: ToSocketAddrs>(addr: To, ssl_context: SslContext) -> hyper::Result<HttpListener> {
        Ok(HttpListener::Https(try!(TcpListener::bind(addr)), Arc::new(ssl_context)))
    }
}

impl NetworkListener for HttpListener {
    type Stream = HttpStream;

    #[inline]
    fn accept(&mut self) -> hyper::Result<HttpStream> {
        match *self {
            HttpListener::Http(ref mut tcp) => Ok(HttpStream::Http(CloneTcpStream(try!(tcp.accept())))),
            HttpListener::Https(ref mut tcp, ref ssl_context) => {
                let stream = CloneTcpStream(try!(tcp.accept()));
                match SslStream::new_server(&**ssl_context, stream) {
                    Ok(ssl_stream) => Ok(HttpStream::Https(ssl_stream)),
                    Err(SslIoError(e)) => {
                        Err(io::Error::new(io::ErrorKind::ConnectionAborted, e).into())
                    },
                    Err(e) => Err(e.into())
                }
            }
        }
    }

    #[inline]
    fn local_addr(&mut self) -> io::Result<SocketAddr> {
        match *self {
            HttpListener::Http(ref mut tcp) => tcp.local_addr(),
            HttpListener::Https(ref mut tcp, _) => tcp.local_addr(),
        }
    }
}


#[doc(hidden)]
pub struct CloneTcpStream(TcpStream);

impl Clone for CloneTcpStream{
    #[inline]
    fn clone(&self) -> CloneTcpStream {
        CloneTcpStream(self.0.try_clone().unwrap())
    }
}

impl Read for CloneTcpStream {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for CloneTcpStream {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// A wrapper around a TcpStream.
#[derive(Clone)]
pub enum HttpStream {
    /// A stream over the HTTP protocol.
    Http(CloneTcpStream),
    /// A stream over the HTTP protocol, protected by SSL.
    Https(SslStream<CloneTcpStream>),
}

impl fmt::Debug for HttpStream {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    match *self {
      HttpStream::Http(_) => write!(fmt, "Http HttpStream"),
      HttpStream::Https(_) => write!(fmt, "Https HttpStream"),
    }
  }
}

impl Read for HttpStream {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match *self {
            HttpStream::Http(ref mut inner) => inner.read(buf),
            HttpStream::Https(ref mut inner) => inner.read(buf)
        }
    }
}

impl Write for HttpStream {
    #[inline]
    fn write(&mut self, msg: &[u8]) -> io::Result<usize> {
        match *self {
            HttpStream::Http(ref mut inner) => inner.write(msg),
            HttpStream::Https(ref mut inner) => inner.write(msg)
        }
    }
    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match *self {
            HttpStream::Http(ref mut inner) => inner.flush(),
            HttpStream::Https(ref mut inner) => inner.flush(),
        }
    }
}

impl NetworkStream for HttpStream {
    #[inline]
    fn peer_addr(&mut self) -> io::Result<SocketAddr> {
        match *self {
            HttpStream::Http(ref mut inner) => inner.0.peer_addr(),
            HttpStream::Https(ref mut inner) => inner.get_mut().0.peer_addr()
        }
    }

    #[inline]
    fn close(&mut self, how: Shutdown) -> io::Result<()> {
        #[inline]
        fn shutdown(tcp: &mut TcpStream, how: Shutdown) -> io::Result<()> {
            use simplesched::net::tcp;
            let how = match how {
                Shutdown::Read => tcp::Shutdown::Read,
                Shutdown::Write => tcp::Shutdown::Write,
                Shutdown::Both => tcp::Shutdown::Both,
            };

            match tcp.shutdown(how) {
                Ok(_) => Ok(()),
                // see https://github.com/hyperium/hyper/issues/508
                Err(ref e) if e.kind() == io::ErrorKind::NotConnected => Ok(()),
                err => err
            }
        }

        match *self {
            HttpStream::Http(ref mut inner) => shutdown(&mut inner.0, how),
            HttpStream::Https(ref mut inner) => shutdown(&mut inner.get_mut().0, how)
        }
    }
}

fn main() {
    env_logger::init().unwrap();

    let matches = App::new("http-echo")
            .version(env!("CARGO_PKG_VERSION"))
            .author("Y. T. Chung <zonyitoo@gmail.com>")
            .arg(Arg::with_name("BIND").short("b").long("bind").takes_value(true).required(true)
                    .help("Listening on this address"))
            .arg(Arg::with_name("THREADS").short("t").long("threads").takes_value(true)
                    .help("Number of threads"))
            .arg(Arg::with_name("KEEPALIVE").short("c").long("keep-alive").takes_value(false)
                    .help("Keep alive"))
            .get_matches();

    let bind_addr = matches.value_of("BIND").unwrap().to_owned();

    let keep_alive = match matches.value_of("KEEPALIVE") {
        Some(..) => true,
        None => false,
    };

    Scheduler::spawn(move|| {
        let mut listener = HttpListener::http(&bind_addr[..]).unwrap();

        loop {
            let mut stream = listener.accept().unwrap();

            if keep_alive {
                Scheduler::spawn(move|| Worker(&echo_keepalive).handle_connection(&mut stream));
            } else {
                Scheduler::spawn(move|| Worker(&echo_close).handle_connection(&mut stream));
            }
        }
    });

    Scheduler::run(matches.value_of("THREADS").unwrap_or("1").parse().unwrap());
}
