use once_cell::sync::Lazy;
use rustls::ServerConfig;
use rustls::crypto::aws_lc_rs::Ticketer;
use rustls::crypto::aws_lc_rs::sign::any_supported_type;
use rustls::server::ServerSessionMemoryCache;
use rustls::{compress::CompressionCache, server::ResolvesServerCert, sign::CertifiedKey};
use std::sync::{Arc, Mutex};
use std::{fs::File, io::BufReader};

pub static CERT_DB: Lazy<Mutex<std::collections::HashMap<String, TlsCollection>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

#[derive(Debug)]
struct ResolveServerCert;

impl ResolvesServerCert for ResolveServerCert {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        match client_hello.server_name() {
            Some(sni) => match CERT_DB.lock() {
                Ok(cert_db) => {
                    if let Some(cert_key) = cert_db.get(sni) {
                        return Some(cert_key.certified_key.clone());
                    }
                    None
                }
                Err(_) => None,
            },
            None => None,
        }
    }
}

pub struct TlsCollection {
    pub certified_key: Arc<rustls::sign::CertifiedKey>,
}

pub async fn get_cert_key(domain: &str) -> Option<CertifiedKey> {
    // todo: read letsencrypt certs from disk

    let cert_file = format!("/etc/letsencrypt/live/{}/fullchain.pem", domain);
    let key_file = format!("/etc/letsencrypt/live/{}/privkey.pem", domain);

    let cert_path = std::path::Path::new(&cert_file);
    let key_path = std::path::Path::new(&key_file);

    if cert_path.exists() && cert_path.is_file() && key_path.exists() && key_path.is_file() {
        let cert_file = &mut BufReader::new(File::open(cert_file).unwrap());
        let private_key_file = &mut BufReader::new(File::open(key_file).unwrap());

        let certs = rustls_pemfile::certs(cert_file)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let private_key = rustls_pemfile::private_key(private_key_file).unwrap();

        if private_key.is_some() {
            let pk = private_key.unwrap().clone_key();
            let certified_key = CertifiedKey {
                cert: certs,
                key: any_supported_type(&pk).unwrap(),
                ocsp: None,
            };

            return Some(certified_key);
        }
    }
    None
}

pub fn create_server_config() -> ServerConfig {
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(ResolveServerCert));

    server_config.ticketer = Ticketer::new().unwrap();
    server_config.session_storage = ServerSessionMemoryCache::new(10024);
    server_config.cert_compression_cache = Arc::new(CompressionCache::new(2038));
    server_config.key_log = Arc::new(rustls::KeyLogFile::new());
    server_config.max_early_data_size = 2048;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    server_config
}

// codenameoyster.ai
// rustatian.me
pub async fn init_cert_in_memory(listen: Vec<String>) -> std::io::Result<()> {
    for domain in listen {
        if let Some(key) = get_cert_key(&domain).await {
            let mut cert_db = CERT_DB.lock().unwrap();
            cert_db.insert(
                domain.to_owned(),
                TlsCollection {
                    certified_key: Arc::new(key),
                },
            );
        }
    }
    Ok(())
}
