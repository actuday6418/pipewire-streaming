use anyhow::Result;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::broadcast;
use wtransport::endpoint::IncomingSession;

async fn handle_connection(
    incoming_session: IncomingSession,
    mut rx: broadcast::Receiver<Vec<u8>>,
) -> Result<()> {
    let session_request = incoming_session.await?;
    let connection = session_request.accept().await?;
    let mut send_stream = connection.open_uni().await?.await?;
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(msg) => send_stream.write_all(&msg).await?,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => eprintln!("WARN: Audio receiver for client {} lagged, {} messages missed. Stream may be distorted.", connection.stable_id(), n),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(())
                }
            }
        }
    }
}

pub fn spawn_webtransport_thread(
    packet_receiver: broadcast::Receiver<Vec<u8>>,
    listen_address: u16,
) -> JoinHandle<()> {
    let handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Couldn't start tokio!");
        runtime.block_on(async move {
            let identity = wtransport::Identity::load_pemfiles("cert.pem", "key.pem")
                .await
                .unwrap();
            println!(
                "{}",
                identity.certificate_chain().as_slice()[0]
                    .hash()
                    .fmt(wtransport::tls::Sha256DigestFmt::BytesArray),
            );
            let config = wtransport::ServerConfig::builder()
                .with_bind_default(listen_address)
                .with_identity(identity)
                .keep_alive_interval(Some(Duration::from_secs(3)))
                .build();

            let server = wtransport::Endpoint::server(config).unwrap();
            loop {
                let incoming_session = server.accept().await;
                tokio::spawn(handle_connection(
                    incoming_session,
                    packet_receiver.resubscribe(),
                ));
            }
        })
    });
    handle
}
