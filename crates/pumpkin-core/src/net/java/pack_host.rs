//! Resource packs served from the game port itself (Paper-style "from this server").
//!
//! An HTTP `GET` on the address players join with is answered with a pack zip a plugin hosted,
//! so no web host or extra port is needed. A Minecraft connection never starts with `GET `
//! (its first packet is a handshake, id 0, or the legacy ping 0xFE), so the two are told apart
//! by peeking at the first bytes. Only checked while a pack is hosted.

use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, PoisonError, RwLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// URL path prefix of hosted packs.
pub const PATH_PREFIX: &str = "/resourcepacks/";

static HOSTED: LazyLock<RwLock<HashMap<String, Arc<[u8]>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// The lowercase hex SHA-1 the client checks a downloaded pack against.
#[must_use]
pub fn sha1_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha1::digest(bytes)
        .iter()
        .fold(String::with_capacity(40), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Hosts a pack zip; returns its SHA-1 (also its key and file name).
pub fn host(zip: Vec<u8>) -> String {
    let sha1 = sha1_hex(&zip);
    HOSTED
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(sha1.clone(), zip.into());
    sha1
}

/// Stops hosting a pack; returns whether it was hosted.
pub fn unhost(sha1: &str) -> bool {
    HOSTED
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&sha1.to_ascii_lowercase())
        .is_some()
}

#[must_use]
pub fn is_hosted(sha1: &str) -> bool {
    HOSTED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(&sha1.to_ascii_lowercase())
}

fn any_hosted() -> bool {
    !HOSTED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .is_empty()
}

/// The URL a player downloads a hosted pack from: `base_url` when set (e.g. behind a proxy or
/// a port forward that changes the port), else the address and port the player joined with.
#[must_use]
pub fn url_for(sha1: &str, base_url: &str, joined_host: &str, joined_port: u16) -> String {
    let sha1 = sha1.to_ascii_lowercase();
    if base_url.is_empty() {
        // Forge and some proxies append data after a NUL to the handshake address.
        let host = joined_host.split('\0').next().unwrap_or_default();
        let host = host.trim_end_matches('.');
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        format!("http://{host}:{joined_port}{PATH_PREFIX}{sha1}.zip")
    } else {
        format!("{}{PATH_PREFIX}{sha1}.zip", base_url.trim_end_matches('/'))
    }
}

/// Whether the connection is an HTTP request rather than a Minecraft client. Waits at most a few
/// seconds for the first bytes.
pub async fn is_http_request(stream: &TcpStream) -> bool {
    if !any_hosted() {
        return false;
    }
    let peek = async {
        let mut buf = [0u8; 4];
        loop {
            let read = stream.peek(&mut buf).await.ok()?;
            if read == 0 || buf[..read] != b"GET "[..read] {
                return Some(false);
            }
            if read == buf.len() {
                return Some(true);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), peek)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

/// The hosted pack a request path names, if any.
fn pack_for_path(path: &str) -> Option<Arc<[u8]>> {
    let path = path.split(['?', '#']).next()?;
    let name = path.strip_prefix(PATH_PREFIX)?;
    let sha1 = name
        .strip_suffix(".zip")
        .unwrap_or(name)
        .to_ascii_lowercase();
    HOSTED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&sha1)
        .cloned()
}

/// Answers one HTTP request with a hosted pack (or 404) and closes the connection.
pub async fn serve(mut stream: TcpStream) {
    let mut request = Vec::with_capacity(512);
    let read_head = async {
        let mut buf = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < 8192 {
            let read = stream.read(&mut buf).await.ok()?;
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buf[..read]);
        }
        Some(())
    };
    if tokio::time::timeout(Duration::from_secs(10), read_head)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return;
    }
    let head = String::from_utf8_lossy(&request);
    let path = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default();
    let response = pack_for_path(path);
    let result = match response {
        Some(zip) => {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                zip.len()
            );
            match stream.write_all(header.as_bytes()).await {
                Ok(()) => stream.write_all(&zip).await,
                Err(error) => Err(error),
            }
        }
        None => {
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
        }
    };
    if result.is_ok() {
        let _ = stream.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_paths() {
        let sha1 = host(b"pack bytes".to_vec());
        assert_eq!(sha1, sha1_hex(b"pack bytes"));
        assert_eq!(sha1.len(), 40);
        assert_eq!(
            url_for(&sha1, "", "play.example.com\0FML3\0", 25565),
            format!("http://play.example.com:25565/resourcepacks/{sha1}.zip")
        );
        assert_eq!(
            url_for(&sha1, "https://cdn.example.com/", "ignored", 1),
            format!("https://cdn.example.com/resourcepacks/{sha1}.zip")
        );
        assert_eq!(
            url_for(&sha1, "", "::1", 25565),
            format!("http://[::1]:25565/resourcepacks/{sha1}.zip")
        );
        assert!(pack_for_path(&format!("/resourcepacks/{sha1}.zip?x=1")).is_some());
        assert!(pack_for_path("/resourcepacks/unknown.zip").is_none());
        assert!(unhost(&sha1));
        assert!(!is_hosted(&sha1));
    }

    #[tokio::test]
    async fn serves_a_hosted_pack_and_tells_minecraft_apart() {
        let sha1 = host(b"zip!".to_vec());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            assert!(is_http_request(&stream).await);
            serve(stream).await;
            let (stream, _) = listener.accept().await.unwrap();
            assert!(!is_http_request(&stream).await);
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(
                format!("GET /resourcepacks/{sha1}.zip HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes(),
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8_lossy(&response);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("zip!"));
        // A handshake: length 16, packet id 0.
        let mut minecraft = TcpStream::connect(addr).await.unwrap();
        minecraft.write_all(&[16, 0, 0xF7, 5]).await.unwrap();
        server.await.unwrap();
        unhost(&sha1);
    }
}
