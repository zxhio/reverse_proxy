use async_std::io;
use async_std::net::{Shutdown, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::task;

fn main() -> io::Result<()> {
    let local_addr = "[::]:8086";
    let upstream_addr: &str = "127.0.0.1:8000";

    task::block_on(serve(local_addr, upstream_addr))?;

    Ok(())
}

async fn serve(addr: &str, upstream_addr: &'static str) -> io::Result<()> {
    let listener = TcpListener::bind(addr).await?;

    println!("Listen on {}", listener.local_addr()?);

    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        let conn = TcpStream::connect(upstream_addr)
            .await
            .map(|upstream| task::spawn(concat_connection(stream, upstream)));

        match conn {
            Ok(_) => {}
            Err(e) => {
                println!("{}", e)
            }
        }
    }

    Ok(())
}

async fn concat_connection(stream: TcpStream, upstream: TcpStream) -> io::Result<()> {
    println!(
        "New connection from {} to {}",
        stream.peer_addr()?.to_string(),
        upstream.peer_addr()?.to_string(),
    );

    let (mut r_stream, mut w_stream) = (stream.clone(), stream.clone());
    let (mut r_upstream, mut w_upstream) = (upstream.clone(), upstream.clone());

    task::spawn(async move {
        match io::copy(&mut r_stream, &mut w_upstream).await {
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

    task::spawn(async move {
        match io::copy(&mut r_upstream, &mut w_stream).await {
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

    Ok(())
}
