//! Локальный потоковый HTTP-сервер (`127.0.0.1:0`) для воспроизведения роликов во время
//! загрузки: WebView2 читает `Range`-запросы из растущего `.part`-файла, не дожидаясь конца
//! скачивания, а после переименования продолжает читать готовый файл из кэша.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Общее состояние активной потоковой загрузки.
pub struct StreamState {
    pub part: PathBuf,
    pub dest: PathBuf,
    pub total: u64,
    pub mime: String,
    pub done: AtomicU64,
    pub finished: AtomicBool,
    pub cancelled: AtomicBool,
}

impl StreamState {
    pub fn new(part: PathBuf, dest: PathBuf, total: u64, ext: &str) -> Arc<Self> {
        let mime = match ext.to_ascii_lowercase().as_str() {
            "webm" => "video/webm",
            "mov" => "video/quicktime",
            "mkv" => "video/x-matroska",
            "ogv" => "video/ogg",
            "gif" => "image/gif",
            "mp3" => "audio/mpeg",
            "m4a" | "aac" => "audio/mp4",
            "ogg" | "oga" | "opus" => "audio/ogg",
            "flac" => "audio/flac",
            "wav" => "audio/wav",
            _ => "video/mp4",
        };
        Arc::new(Self {
            part,
            dest,
            total: total.max(1),
            mime: mime.to_owned(),
            done: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        })
    }

    fn current_file(&self) -> Option<std::fs::File> {
        std::fs::File::open(&self.dest)
            .or_else(|_| std::fs::File::open(&self.part))
            .ok()
    }
}

/// Разбирает заголовок `Range: bytes=start-end` (или `bytes=start-`).
pub fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let start = a.trim().parse::<u64>().ok()?;
    if start >= total {
        return None;
    }
    let end = if b.trim().is_empty() {
        total - 1
    } else {
        b.trim().parse::<u64>().ok()?.min(total - 1)
    };
    (end >= start).then_some((start, end))
}

/// Поднимает одноразовый локальный сервер на `127.0.0.1:0` и возвращает адрес для плеера.
pub async fn start(state: Arc<StreamState>) -> Option<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await.ok()?;
    let port = listener.local_addr().ok()?.port();
    tokio::spawn(async move {
        // Живёт, пока ролик скачивается и до 5 минут после завершения (пока его смотрят).
        let idle_limit = std::time::Duration::from_secs(300);
        loop {
            if state.cancelled.load(Ordering::Relaxed) {
                break;
            }
            let accept = tokio::time::timeout(idle_limit, listener.accept()).await;
            let Ok(Ok((stream, _))) = accept else { break };
            let st = state.clone();
            tokio::spawn(async move {
                let _ = serve_conn(stream, st).await;
            });
        }
    });
    Some(format!("http://127.0.0.1:{port}/video")) // allow-host
}

async fn serve_conn(mut sock: tokio::net::TcpStream, st: Arc<StreamState>) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let n = sock.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    let is_head = first_line.starts_with("HEAD ");
    let is_options = first_line.starts_with("OPTIONS ");
    if is_options {
        let cors = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Headers: Range\r\nConnection: close\r\n\r\n";
        sock.write_all(cors.as_bytes()).await?;
        let _ = sock.shutdown().await;
        return Ok(());
    }
    let mut range_hdr = None;
    for line in req.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case("range")
        {
            range_hdr = Some(v.trim().to_owned());
            break;
        }
    }
    let total = st.total;
    let (status, start, end) = match range_hdr.as_deref().and_then(|h| parse_range(h, total)) {
        Some((s, e)) => ("206 Partial Content", s, e),
        None => ("200 OK", 0, total - 1),
    };
    let len = end - start + 1;
    let headers = if status.starts_with("206") {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {}\r\nContent-Length: {len}\r\nContent-Range: bytes {start}-{end}/{total}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: Content-Range, Accept-Ranges, Content-Length\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            st.mime
        )
    } else {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: Content-Range, Accept-Ranges, Content-Length\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            st.mime
        )
    };
    sock.write_all(headers.as_bytes()).await?;
    if is_head {
        let _ = sock.shutdown().await;
        return Ok(());
    }

    let mut pos = start;
    let mut chunk = vec![0u8; 64 * 1024];
    while pos <= end {
        if st.cancelled.load(Ordering::Relaxed) {
            break;
        }
        // Ждём, пока загрузчик допишет данные до `pos` (если файл ещё в процессе).
        let mut waited = 0u32;
        while !st.finished.load(Ordering::Acquire)
            && st.done.load(Ordering::Acquire) <= pos
            && !st.cancelled.load(Ordering::Relaxed)
            && waited < 400
        {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            waited += 1;
        }
        let avail = if st.finished.load(Ordering::Acquire) {
            end + 1
        } else {
            st.done.load(Ordering::Acquire).min(end + 1)
        };
        if avail <= pos {
            break;
        }
        let want = ((avail - pos) as usize).min(chunk.len());
        let Some(mut file) = st.current_file() else { break };
        if file.seek(SeekFrom::Start(pos)).is_err() {
            break;
        }
        let got = file.read(&mut chunk[..want])?;
        if got == 0 {
            if st.finished.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            continue;
        }
        sock.write_all(&chunk[..got]).await?;
        pos += got as u64;
    }
    let _ = sock.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_ranges() {
        assert_eq!(parse_range("bytes=0-1023", 5000), Some((0, 1023)));
        assert_eq!(parse_range("bytes=100-", 5000), Some((100, 4999)));
        assert_eq!(parse_range("bytes=0-99999", 500), Some((0, 499)));
        assert_eq!(parse_range("bytes=500-600", 500), None);
        assert_eq!(parse_range("items=0-1", 500), None);
    }

    #[tokio::test]
    async fn streams_partial_range_from_local_file() {
        let dir = std::env::temp_dir().join("rg-stream-test");
        let _ = std::fs::create_dir_all(&dir);
        let part = dir.join("clip.mp4.part");
        let dest = dir.join("clip.mp4");
        let data: Vec<u8> = (0..1024).map(|i| (i % 251) as u8).collect();
        std::fs::write(&part, &data).unwrap();
        let st = StreamState::new(part.clone(), dest.clone(), data.len() as u64, "mp4");
        st.done.store(data.len() as u64, Ordering::Release);
        st.finished.store(true, Ordering::Release);
        let url = start(st.clone()).await.unwrap();
        let port: u16 = url.split(':').nth(2).unwrap().split('/').next().unwrap().parse().unwrap();
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client
            .write_all(b"GET /video HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=10-29\r\n\r\n")
            .await
            .unwrap();
        let mut resp = Vec::new();
        client.read_to_end(&mut resp).await.unwrap();
        let split = resp.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&resp[..split]);
        assert!(head.contains("206 Partial Content"));
        assert!(head.contains("Content-Range: bytes 10-29/1024"));
        assert_eq!(&resp[split + 4..], &data[10..30]);
        st.cancelled.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&part);
    }
}
