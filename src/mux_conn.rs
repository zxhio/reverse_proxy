use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

// Trait that defines the methods needed for a multiplexed connection.
trait MuxConn: Read + Write {
    fn local_addr(&self) -> io::Result<SocketAddr>;

    fn peer_addr(&self) -> io::Result<SocketAddr>;

    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()>;

    fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()>;

    fn shutdown(&self, how: Shutdown) -> io::Result<()>;
}

// Implement the MuxConn trait for TcpStream.
impl MuxConn for TcpStream {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.local_addr()
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.peer_addr()
    }

    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(dur)
    }

    fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.set_write_timeout(dur)
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.shutdown(how)
    }
}

struct TcpMuxConn {}

fn hh() -> impl Read {
    let x = TcpStream::connect("addr").unwrap();
    return x;
}


fn h2() {
}