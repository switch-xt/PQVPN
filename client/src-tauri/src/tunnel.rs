use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;

pub async fn start_proxy(local_udp_port: u16, remote_tcp_addr: &str) -> Result<()> {
    let local_udp_addr = format!("127.0.0.1:{}", local_udp_port);
    let udp_socket = UdpSocket::bind(&local_udp_addr).await.context("bind UDP")?;
    let udp_socket = Arc::new(udp_socket);

    let tcp_stream = TcpStream::connect(remote_tcp_addr).await.context("connect TCP")?;
    let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

    let client_addr: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));

    let udp_socket_clone = Arc::clone(&udp_socket);
    let client_addr_clone = Arc::clone(&client_addr);

    // Task 1: Read from TCP, write to UDP
    tokio::spawn(async move {
        loop {
            let mut len_buf = [0u8; 2];
            if let Err(_) = tcp_read.read_exact(&mut len_buf).await {
                break;
            }
            let len = u16::from_le_bytes(len_buf) as usize;
            
            let mut payload = vec![0u8; len];
            if let Err(_) = tcp_read.read_exact(&mut payload).await {
                break;
            }

            let target = *client_addr_clone.lock().await;
            if let Some(addr) = target {
                let _ = udp_socket_clone.send_to(&payload, addr).await;
            }
        }
    });

    // Task 2: Read from UDP, write to TCP
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        loop {
            if let Ok((len, addr)) = udp_socket.recv_from(&mut buf).await {
                *client_addr.lock().await = Some(addr);
                
                let mut tcp_data = Vec::with_capacity(len + 2);
                tcp_data.extend_from_slice(&(len as u16).to_le_bytes());
                tcp_data.extend_from_slice(&buf[..len]);
                
                if let Err(_) = tcp_write.write_all(&tcp_data).await {
                    break;
                }
            } else {
                break;
            }
        }
    });

    Ok(())
}
