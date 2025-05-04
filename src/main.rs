use axum::{Router, http::Request};
use futures_util::pin_mut;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::{sync::Arc, vec};
use tokio::net::TcpListener;
use tokio_rustls::{TlsAcceptor, server};
use tower::{Service, ServiceBuilder};
use tower_http::{
    compression::CompressionLayer, decompression::RequestDecompressionLayer, services::ServeDir,
};
use tracing::{debug, error, warn};
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
    let tcp_listener = TcpListener::bind("0.0.0.0:443").await.unwrap();

    let app = Router::new();

    pin_mut!(tcp_listener);

    loop {
        let tower_service: Router = app.clone();
        let tls_acceptor: TlsAcceptor = tls_acceptor.clone();

        // Accept a new TCP connection
        let res: Result<(tokio::net::TcpStream, std::net::SocketAddr), std::io::Error> =
            tcp_listener.accept().await;

        // Wait for new tcp connection
        let (cnx, addr) = match res {
            Ok((cnx, addr)) => (cnx, addr),
            Err(e) => {
                error!("error accepting connection: {}", e);
                continue;
            }
        };

        tokio::spawn(async move {
            // Wait for tls handshake to happen
            let Ok(stream) = tls_acceptor.accept(cnx).await else {
                error!("error during tls handshake connection from {}", addr);
                return;
            };

            // io + serve_connection
            let (_, session) = stream.get_ref();
            let sn: Option<String> = session.server_name().map(String::from);
            if sn.is_none() {
                error!("error getting server name from connection from {}", addr);
                return;
            }

            // safe unwrap
            let sn = sn.unwrap();

            let tower_service = match sn.as_str() {
                "codenameoyster.ai" => {
                    debug!("Serving codenameoyster.ai");
                    tower_service
                        .fallback_service(ServeDir::new("/var/www/codenameoyster.ai/html"))
                        .layer(
                            ServiceBuilder::new()
                                .layer(RequestDecompressionLayer::new())
                                .layer(CompressionLayer::new()),
                        )
                }
                "rustatian.me" => {
                    debug!("Serving rustatian.me");
                    tower_service
                        .fallback_service(ServeDir::new("/var/www/rustatian.me/html"))
                        .layer(
                            ServiceBuilder::new()
                                .layer(RequestDecompressionLayer::new())
                                .layer(CompressionLayer::new()),
                        )
                }
                _ => {
                    error!("error getting server name from connection from {}", addr);
                    return;
                }
            };

            let stream: TokioIo<server::TlsStream<tokio::net::TcpStream>> = TokioIo::new(stream);

            let hyper_service = hyper::service::service_fn(move |request: Request<Incoming>| {
                tower_service.clone().call(request)
            });

            let ret: Result<(), hyper::Error> =
                hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                    .serve_connection(stream, hyper_service)
                    .await;

            if let Err(err) = ret {
                warn!("error serving connection from {}: {}", addr, err);
            }
        });
    }
}

async fn handler() -> &'static str {
    "Hello, World!"
}
