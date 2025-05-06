use anyhow::Result;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::watch;
use wtransport::endpoint::IncomingSession;

async fn handle_connection(
    incoming_session: IncomingSession,
    mut rx: watch::Receiver<Vec<u8>>,
) -> Result<()> {
    let session_request = incoming_session.await?;
    let connection = session_request.accept().await?;
    let mut send_stream = connection.open_uni().await?.await?;
    loop {
        tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() {return Ok(())}
                let packet = rx.borrow_and_update().clone();
                send_stream.write_all(&packet).await?;
            }
        }
    }
}

pub fn spawn_webtransport_thread(
    packet_receiver: watch::Receiver<Vec<u8>>,
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
            let config = wtransport::ServerConfig::builder()
                .with_bind_default(listen_address)
                .with_identity(identity)
                .keep_alive_interval(Some(Duration::from_secs(3)))
                .build();

            let server = wtransport::Endpoint::server(config).unwrap();
            loop {
                let incoming_session = server.accept().await;
                tokio::spawn(handle_connection(incoming_session, packet_receiver.clone()));
            }
        })
    });
    handle
}
