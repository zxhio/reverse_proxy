use async_std::io;
use async_std::net::{Shutdown, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::task;

fn main() -> io::Result<()> {
    let local_addr = "[::]:8086";
    let upstream_addr = "127.0.0.1:8000";

    task::block_on(serve(local_addr, upstream_addr))?;

    Ok(())
}

async fn serve(addr: &str, upstream_addr: &'static str) -> io::Result<()> {
    let listener = TcpListener::bind(addr).await?;

    println!("Listen on {}", listener.local_addr()?);

    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        TcpStream::connect(upstream_addr)
            .await
            .map(|upstream| task::spawn(concat_connection(stream, upstream)))?;
    }

    Ok(())
}

async fn concat_connection(stream: TcpStream, upstream: TcpStream) -> io::Result<()> {
    let (mut r_stream, mut w_stream) = (stream.clone(), stream.clone());
    let (mut r_upstream, mut w_upstream) = (upstream.clone(), upstream.clone());

    let to = task::spawn(async move {
        let _ = io::copy(&mut r_stream, &mut w_upstream).await;
        let _ = r_stream.shutdown(Shutdown::Both);
        let _ = w_upstream.shutdown(Shutdown::Both);
    });

    let from = task::spawn(async move {
        let _ = io::copy(&mut r_upstream, &mut w_stream).await;
        let _ = r_upstream.shutdown(Shutdown::Both);
        let _ = w_stream.shutdown(Shutdown::Both);
    });

    to.await;
    from.await;

    Ok(())
}
