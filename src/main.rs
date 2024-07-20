use clap::{Arg, Command};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use tokio::io::{self, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket, TcpStream};

#[derive(Serialize, Deserialize, Debug)]
struct AddrPair {
    listen_addr: String,
    remote_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_addr: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Config {
    addr_pair_list: Vec<AddrPair>,
}

const DEFAULT_CONF_PATH: &str = "/etc/reverse_proxy/config.json";
const DEFAULT_LOG_PATH: &str = "/var/log/reverse_proxy.log";

#[derive(Debug)]
pub enum ReverseProxyError {
    NoSuchAddressList,
    NoSuchAddress(&'static str),
    InvalidAddress(&'static str, String),
}

impl fmt::Display for ReverseProxyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReverseProxyError::NoSuchAddressList => write!(f, "No such address list"),
            ReverseProxyError::NoSuchAddress(addr_type) => {
                write!(f, "No such {} address", addr_type)
            }
            ReverseProxyError::InvalidAddress(addr_type, addr) => {
                if addr_type.is_empty() {
                    write!(f, "Invalid address '{}'", addr)
                } else {
                    write!(f, "Invalid {}address '{}'", addr_type, addr)
                }
            }
        }
    }
}

impl std::error::Error for ReverseProxyError {}

impl From<ReverseProxyError> for io::Error {
    fn from(err: ReverseProxyError) -> io::Error {
        match err {
            ReverseProxyError::NoSuchAddressList => {
                std::io::Error::new(io::ErrorKind::InvalidData, "No such address list")
            }
            ReverseProxyError::NoSuchAddress(addr_type) => std::io::Error::new(
                io::ErrorKind::InvalidData,
                format!("No such {} address", addr_type),
            ),
            ReverseProxyError::InvalidAddress(addr_type, addr) => std::io::Error::new(
                io::ErrorKind::InvalidData,
                if addr_type.is_empty() {
                    format!("Invalid address '{}'", addr)
                } else {
                    format!("Invalid {} address '{}'", addr_type, addr)
                },
            ),
        }
    }
}

struct LoggerWriter {
    pub f: file_rotate::FileRotate<file_rotate::suffix::AppendTimestamp>,
}

impl LoggerWriter {
    fn new(path: &str) -> Self {
        let f = file_rotate::FileRotate::new(
            path,
            file_rotate::suffix::AppendTimestamp::default(
                file_rotate::suffix::FileLimit::MaxFiles(10),
            ),
            file_rotate::ContentLimit::Bytes(1024 * 1024 * 10),
            file_rotate::compression::Compression::None,
            #[cfg(unix)]
            None,
        );
        LoggerWriter { f }
    }
}

impl Write for LoggerWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.f.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let matches = Command::new("reverse_proxy")
        .version("0.1.0")
        .author("definezxh@163.com")
        .about("reverse_proxy - Reverse Proxy")
        .arg(
            Arg::new("listen-addr")
                .short('l')
                .long("listen-addr")
                .value_name("LISTEN_ADDR")
                .help("Proxy listen address, format 'ip:port' or 'port'")
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("remote-addr")
                .short('r')
                .long("remote-addr")
                .value_name("REMOTE_ADDR")
                .help("Proxy upstream conn remote address, format 'ip:port'")
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("local-addr")
                .long("local-addr")
                .value_name("LOCAL_ADDR")
                .help("Proxy upstream conn local address, format 'ip:port' or 'ip'")
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("CONFIG_PATH")
                .help(format!("Config path, default {}", DEFAULT_CONF_PATH))
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("dump-config")
                .long("dump-config")
                .value_name("CONFIG_TYPE")
                .help("Dump config for 'default' or 'env'")
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("log-path")
                .long("log-path")
                .value_name("LOG_PATH")
                .help(format!("Log path, default {}", DEFAULT_LOG_PATH))
                .num_args(1)
                .required(false),
        )
        .get_matches();

    let listen_addr_opt = matches.get_one::<String>("listen-addr");
    let remote_addr_opt = matches.get_one::<String>("remote-addr");
    let local_addr_opt = matches.get_one::<String>("local-addr");
    let conf_path_opt = matches.get_one::<String>("config");
    let dump_conf_opt = matches.get_one::<String>("dump-config");
    let log_path_opt = matches.get_one::<String>("log-path");

    let mut addr_pairs = Vec::<AddrPair>::new();

    let empty_str = String::new();
    let listen_addr = listen_addr_opt.unwrap_or(&empty_str).to_string();
    let remote_addr = remote_addr_opt.unwrap_or(&empty_str).to_string();

    let mut is_command_line = false;
    // From command args
    if !listen_addr.is_empty() || !remote_addr.is_empty() {
        if listen_addr.is_empty() {
            return Err(ReverseProxyError::NoSuchAddress("listen").into());
        }
        if remote_addr.is_empty() {
            return Err(ReverseProxyError::NoSuchAddress("remote").into());
        }

        is_command_line = true;
        addr_pairs.push(AddrPair {
            listen_addr,
            remote_addr,
            local_addr: local_addr_opt.and_then(|s| Some(s.to_string())),
        });
    } else {
        // From config.json
        let conf_path_def = String::from(DEFAULT_CONF_PATH);
        let conf_path = conf_path_opt.unwrap_or(&conf_path_def).to_string();
        let mut file = File::open(conf_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let mut c: Config = serde_json::from_str(&contents)?;
        addr_pairs.append(&mut c.addr_pair_list);
    }

    if let Some(dump_opt) = dump_conf_opt {
        dump_config(dump_opt.to_string(), addr_pairs)?;
        return Ok(());
    }

    set_logging(log_path_opt, is_command_line);
    info!("=== Reverse Proxy start ===");

    let addresses = to_sock_addresses(&addr_pairs)?;

    let mut handles = vec![];
    for pair in addresses {
        let handle = tokio::spawn(serve(pair.listen_addr, pair.remote_addr, pair.local_addr));
        handles.push(handle);
    }
    for handle in handles {
        handle.await??;
    }

    Ok(())
}

//  User  <---- uconn ---->   ReverseProxy   <---- rconn ---->  Upstream
//      uaddr_r        uaddr_l             raddr_l         raddr_r
async fn serve(
    listen_addr: SocketAddr,
    remote_addr: SocketAddr,
    local_addr: Option<SocketAddr>,
) -> io::Result<()> {
    let listener = TcpListener::bind(&listen_addr).await?;
    info!("Listen on addr={}", listen_addr);

    loop {
        match listener.accept().await {
            Ok((mut uconn, uaddr_r)) => {
                let uaddr_l = match uconn.local_addr() {
                    Ok(addr) => addr,
                    Err(_) => remote_addr,
                };

                info!(
                    "New conn from={} via={} to={}",
                    uaddr_r, uaddr_l, remote_addr
                );

                // match TcpStream::connect(remote_addr).await {
                match connect_with(local_addr, remote_addr).await {
                    Ok(mut rconn) => {
                        let raddr_l: String = match rconn.local_addr() {
                            Ok(addr) => addr.to_string(),
                            Err(_) => String::new(),
                        };
                        let remote_addr = remote_addr.to_string();
                        tokio::spawn(async move {
                            match io::copy_bidirectional(&mut uconn, &mut rconn).await {
                                Ok((in_n, out_n)) => {
                                    info!(
                                        "Close conn from={} via={}({}) to={} in_bytes={} out_bytes={}",
                                        uaddr_r, uaddr_l, raddr_l, remote_addr, in_n, out_n
                                    )
                                }
                                Err(e) => {
                                    error!(
                                        "Fail to proxy from={} via={}({}) to={} err={}",
                                        uaddr_r, uaddr_l, raddr_l, remote_addr, e,
                                    )
                                }
                            }
                            let _ = uconn.shutdown().await;
                            let _ = rconn.shutdown().await;
                        });
                    }
                    Err(e) => {
                        let _ = uconn.shutdown().await;
                        error!("Fail to connect addr={} err={}", remote_addr, e);
                    }
                }
            }
            Err(e) => {
                error!("Fail to accept err={}", e);
            }
        }
    }
}

fn dump_config(dump_opt: String, addr_pairs: Vec<AddrPair>) -> io::Result<()> {
    let mut pairs = Vec::<AddrPair>::new();
    if dump_opt == "default" {
        pairs.push(AddrPair {
            listen_addr: String::from("<REMOTE_ADDR>"),
            remote_addr: String::from("<LISTEN_ADDR>"),
            local_addr: None,
        });
    } else if dump_opt == "env" {
        for pair in addr_pairs {
            pairs.push(pair);
        }
    }

    let c = Config {
        addr_pair_list: pairs,
    };
    let data = serde_json::to_string_pretty(&c)?;
    println!("{}", data);

    Ok(())
}

fn set_logging(log_path_opt: Option<&String>, is_command_line: bool) {
    let env = env_logger::Env::new().filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
    let mut builder = env_logger::Builder::from_env(env);

    if !is_command_line {
        let log_path_def = String::from(DEFAULT_LOG_PATH);
        let log_path = log_path_opt.unwrap_or(&log_path_def);
        let logger_writer = LoggerWriter::new(log_path);
        builder.target(env_logger::Target::Pipe(Box::new(logger_writer)));
    }

    builder
        .format_level(true)
        .format_timestamp_millis()
        .format_target(false)
        .init();
}

struct SocketAddrPair {
    listen_addr: SocketAddr,
    remote_addr: SocketAddr,
    local_addr: Option<SocketAddr>,
}

fn to_sock_addresses(pairs: &Vec<AddrPair>) -> Result<Vec<SocketAddrPair>, ReverseProxyError> {
    let mut addresses = vec![];
    for pair in pairs {
        let listen_addr = to_sock_address(&pair.listen_addr)?;
        if listen_addr.port() == 0 {
            return Err(ReverseProxyError::InvalidAddress(
                "listen",
                pair.remote_addr.to_string(),
            ));
        }

        let raddr = to_sock_address(&pair.remote_addr)?;
        if raddr.port() == 0 || raddr.ip().is_unspecified() {
            return Err(ReverseProxyError::InvalidAddress(
                "remote",
                pair.remote_addr.to_string(),
            ));
        }

        let local_addr_opt = match &pair.local_addr {
            Some(s) => Some(to_sock_address(&s)?),
            None => None,
        };

        addresses.push(SocketAddrPair {
            listen_addr,
            remote_addr: raddr,
            local_addr: local_addr_opt,
        })
    }
    Ok(addresses)
}

fn to_sock_address(s: &str) -> Result<SocketAddr, ReverseProxyError> {
    // Try to parse the full address
    if let Ok(addr) = SocketAddr::from_str(s) {
        return Ok(addr);
    }

    // If full address parsing fails, try to parse as a port number
    if let Ok(port) = s.parse::<u16>() {
        // Create a SocketAddrV4 using 0.0.0.0 as the default IP address
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
        return Ok(socket_addr);
    }

    // Only ip
    if let Ok(ip) = IpAddr::from_str(s) {
        return Ok(SocketAddr::new(ip, 0));
    }

    // Return an error if neither parsing succeeds
    Err(ReverseProxyError::InvalidAddress("", s.to_string()))
}

async fn connect_with(laddr: Option<SocketAddr>, raddr: SocketAddr) -> io::Result<TcpStream> {
    let socket = if raddr.is_ipv6() {
        TcpSocket::new_v6()?
    } else {
        TcpSocket::new_v4()?
    };

    if let Some(addr) = laddr {
        socket.bind(addr)?;
    }
    Ok(socket.connect(raddr).await?)
}
