use axum::{Router, http::Request, routing::get};
use futures_util::pin_mut;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use openssl::ssl::{Ssl, SslAcceptor, SslFiletype, SslMethod};
use std::{path::PathBuf, pin::Pin, sync::Arc, vec};
use tokio::net::TcpListener;
use tokio_openssl::SslStream;
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod sni;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // codenameoyster.ai-0001
    // rustatian.me
    _ = sni::init_cert_in_memory(vec![
        "codenameoyster.ai".to_string(),
        "rustatian.me".to_string(),
    ])
    .await;

    let config = Arc::new(sni::create_server_config());
    let tls_acceptor = TlsAcceptor::from(config);
    let bind = "0.0.0.0:3000";
    let tcp_listener = TcpListener::bind(bind).await.unwrap();
    info!("HTTPS server listening on {bind}. To contact curl -k https://localhost:3000");
    let app = Router::new().route("/", get(handler));

    pin_mut!(tcp_listener);
    loop {
        let tower_service = app.clone();
        let tls_acceptor = tls_acceptor.clone();

        // Wait for new tcp connection
        let (cnx, addr) = tcp_listener.accept().await.unwrap();

        tokio::spawn(async move {
            // Wait for tls handshake to happen
            let Ok(stream) = tls_acceptor.accept(cnx).await else {
                error!("error during tls handshake connection from {}", addr);
                return;
            };

            let stream = TokioIo::new(stream);

            let hyper_service = hyper::service::service_fn(move |request: Request<Incoming>| {
                tower_service.clone().call(request)
            });

            let ret = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                .serve_connection(stream, hyper_service)
                .await;
            // let ret = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
            //     .serve_connection_with_upgrades(stream, hyper_service)
            //     .await;

            if let Err(err) = ret {
                warn!("error serving connection from {}: {}", addr, err);
            }
        });
    }
}

async fn handler() -> &'static str {
    "Hello, World!"
}
