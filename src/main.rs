use clap::{Arg, Command};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::process::exit;
use tokio::io::{self, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Serialize, Deserialize)]
struct AddrPair {
    remote_addr: String,
    listen_addr: String,
}

#[derive(Serialize, Deserialize)]
struct Config {
    addr_pair_list: Vec<AddrPair>,
}

const DEFAULT_CONF_PATH: &str = "/etc/reverse_proxy/config.json";
const DEFAULT_LOG_PATH: &str = "/var/log/reverse_proxy.log";

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
                .help("Proxy listen address, format ip:port")
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("remote-addr")
                .short('r')
                .long("remote-addr")
                .value_name("REMOTE_ADDR")
                .help("Proxy remote address, format ip:port")
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
    let conf_path_opt = matches.get_one::<String>("config");
    let dump_conf_opt = matches.get_one::<String>("dump-config");
    let log_path_opt = matches.get_one::<String>("log-path");

    let mut listen_addr = String::new();
    let mut remote_addr = String::new();
    if let Some(addr) = listen_addr_opt {
        listen_addr = addr.to_string();
    }
    if let Some(addr) = remote_addr_opt {
        remote_addr = addr.to_string();
    }

    let mut addr_pairs = Vec::<AddrPair>::new();

    // From command args
    if !listen_addr.is_empty() && !remote_addr.is_empty() {
        addr_pairs.push(AddrPair {
            listen_addr,
            remote_addr,
        });

        let env = env_logger::Env::new().filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
        env_logger::Builder::from_env(env)
            .format_level(true)
            .format_timestamp_millis()
            .format_target(false)
            .init();
    } else {
        // From config.json
        let conf_path_def = String::from(DEFAULT_CONF_PATH);
        let conf_path = conf_path_opt.unwrap_or(&conf_path_def).to_string();
        let mut file = File::open(conf_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let mut c: Config = serde_json::from_str(&contents)?;
        addr_pairs.append(&mut c.addr_pair_list);

        let log_path_def = String::from(DEFAULT_LOG_PATH);
        let log_path = log_path_opt.unwrap_or(&log_path_def);
        let logger_writer = LoggerWriter::new(log_path);
        let env = env_logger::Env::new().filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
        env_logger::Builder::from_env(env)
            .format_level(true)
            .format_timestamp_millis()
            .format_target(false)
            .target(env_logger::Target::Pipe(Box::new(logger_writer)))
            .init();
    }

    if let Some(dump_opt) = dump_conf_opt {
        return dump_config(dump_opt.to_string(), addr_pairs);
    }

    if addr_pairs.is_empty() {
        error!("No such proxy addr pairs");
        exit(2);
    }

    info!("=== Reverse Proxy start ===");

    let mut handles = vec![];
    for pair in addr_pairs {
        let (listen_addr, remote_addr) = (pair.listen_addr, pair.remote_addr);
        let handle = tokio::spawn(serve(listen_addr, remote_addr));
        handles.push(handle);
    }
    for handle in handles {
        handle.await??;
    }

    Ok(())
}

//  A  <--- uconn --->   B   <--- rconn --->  C
// uaddr_r       uaddr_l   raddr_l         raddr_r

async fn serve(listen_addr: String, remote_addr: String) -> io::Result<()> {
    let listener = TcpListener::bind(&listen_addr).await?;
    info!("Listen on {}", listen_addr);

    loop {
        match listener.accept().await {
            Ok((mut uconn, uaddr_r)) => {
                let uaddr_l: String = match uconn.local_addr() {
                    Ok(addr) => addr.to_string(),
                    Err(_) => remote_addr.clone(),
                };

                info!(
                    "New conn from={} via={} to={}",
                    uaddr_r, uaddr_l, remote_addr
                );

                match TcpStream::connect(&remote_addr).await {
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
            remote_addr: String::from("<LISTEN_ADDR>"),
            listen_addr: String::from("<REMOTE_ADDR>"),
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
