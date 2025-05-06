use crate::HTTP_PORT;
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use image::DynamicImage;
use image::Luma;
use std::{net::SocketAddr, path::PathBuf, thread::JoinHandle};
use tower_http::services::ServeDir;
use viuer::{Config, print};

pub fn spawn_http_thread() -> JoinHandle<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    print_how_to_connect();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Couldn't start tokio!");
        runtime.block_on(async move {
            let config =
                RustlsConfig::from_pem_file(PathBuf::from("cert.pem"), PathBuf::from("key.pem"))
                    .await
                    .expect("Certificate files not found!");
            let static_files_path = PathBuf::from("web");
            let static_service = ServeDir::new(static_files_path);
            let app = Router::new().fallback_service(static_service);
            let addr = SocketAddr::from(([0, 0, 0, 0], HTTP_PORT));
            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service())
                .await
                .expect("HTTP server failed");
        })
    })
}

fn print_how_to_connect() {
    let maybe_addr = local_ip_address::local_ip().ok();
    let maybe_url = maybe_addr.map(|addr| format!("https://{addr}:{HTTP_PORT}"));
    let maybe_qr = maybe_url
        .clone()
        .and_then(|url| qrcode::QrCode::new(url).ok())
        .map(|qr| qr.render::<Luma<u8>>().module_dimensions(1, 1).build());
    let conf = Config {
        absolute_offset: false,
        ..Default::default()
    };
    println!(
        "Connect to: {}",
        maybe_url.unwrap_or(String::from("I don't know :("))
    );
    if let Some(qr) = maybe_qr {
        let img = DynamicImage::ImageLuma8(qr);
        print(&img, &conf).expect("Image printing failed.");
    }
}
