use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;

fn main() -> std::io::Result<()> {
    let local_addr = "[::]:8086";
    let upstream_addr = "127.0.0.1:8000";

    serve(local_addr, upstream_addr)?;

    Ok(())
}

fn serve(addr: &str, upstream_addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;

    println!("Listen on {}", listener.local_addr()?);

    for stream in listener.incoming() {
        let stream = stream?;
        let conn = TcpStream::connect(upstream_addr)
            .map(|upstream| thread::spawn(move || concat_connection(stream, upstream)));

        match conn {
            Ok(_) => {}
            Err(e) => {
                println!("Fail to proxy stream, {}", e)
            }
        }
    }

    Ok(())
}

fn concat_connection(stream: TcpStream, upstream: TcpStream) -> std::io::Result<()> {
    println!(
        "New connection from {} to {}",
        stream.peer_addr()?.to_string(),
        upstream.peer_addr()?.to_string(),
    );

    let (mut r_stream, mut w_stream) = (stream.try_clone()?, stream.try_clone()?);
    let (mut r_upstream, mut w_upstream) = (upstream.try_clone()?, upstream.try_clone()?);

    let j1 = thread::spawn(move || {
        match std::io::copy(&mut r_stream, &mut w_upstream) {
            Ok(n) => {
                println!("Copy {} byte to upstream", n)
            }
            Err(e) => {
                println!("Fail to copy from stream, {}", e)
            }
        }

        match w_upstream.shutdown(Shutdown::Both) {
            Ok(_) => {}
            Err(e) => {
                println!("Fail to shutdown stream, {}", e)
            }
        }
    });

    let j2 = thread::spawn(move || {
        match std::io::copy(&mut r_upstream, &mut w_stream) {
            Ok(n) => {
                println!("Copy {} byte from upstream", n)
            }
            Err(e) => {
                println!("Fail to copy to stream, {}", e)
            }
        }

        match w_stream.shutdown(Shutdown::Both) {
            Ok(_) => {}
            Err(e) => {
                println!("Fail to shutdown stream, {}", e)
            }
        }
    });

    j1.join().unwrap();
    j2.join().unwrap();

    Ok(())
}
