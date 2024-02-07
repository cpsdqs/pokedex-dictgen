use anyhow::bail;
use reqwest::{blocking::Client, header::HeaderMap, Method};
use std::{fs, io, path::PathBuf, sync::Mutex};

#[derive(Debug)]
pub struct Fetcher {
    dir: PathBuf,
    client: Mutex<Client>,
}

impl Fetcher {
    pub fn new(dir: PathBuf) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("Sec-Fetch-Site", "none".parse().unwrap());
        headers.insert("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.3 Safari/605.1.15".parse().unwrap());
        headers.insert("Accept-Language", "en-US,en;q=0.9".parse().unwrap());

        Self {
            dir,
            client: Mutex::new(Client::builder().default_headers(headers).build().unwrap()),
        }
    }

    pub fn get(&self, url: &str, document: bool) -> anyhow::Result<Vec<u8>> {
        let cache_path = self.dir.join(url.replace('/', "~"));

        match fs::read(&cache_path) {
            Ok(data) => Ok(data),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let client = self.client.lock().unwrap();
                eprintln!("\x1b[32mfetching {url}\x1b[m");
                std::thread::sleep(std::time::Duration::from_millis(500));

                let res = if document {
                    client
                        .request(Method::GET, url)
                        .header(
                            "Accept",
                            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                        )
                        .header("Sec-Fetch-Dest", "document")
                        .header("Sec-Fetch-Mode", "navigate")
                        .header("Sec-Fetch-Site", "none")
                        .send()?
                } else {
                    client
                        .request(Method::GET, url)
                        .header("Accept", "image/webp,image/avif,image/jxl,image/heic,image/heic-sequence,video/*;q=0.8,image/png,image/svg+xml,image/*;q=0.8,*/*;q=0.5")
                        .header("Sec-Fetch-Dest", "image")
                        .header("Sec-Fetch-Mode", "no-cors")
                        .header("Sec-Fetch-Site", "same-site")
                        .header("Referer", "https://bulbapedia.bulbagarden.net/")
                        .send()?
                };

                if !res.status().is_success() {
                    let status = res.status();
                    if let Ok(data) = res.text() {
                        bail!(
                            "failed to fetch {url}: got {}\n{}...",
                            status,
                            data.chars().take(1000).collect::<String>()
                        );
                    } else {
                        bail!("failed to fetch {url}: got {}", status);
                    }
                }
                let data = res.bytes()?.to_vec();

                fs::write(cache_path, &data)?;

                Ok(data)
            }
            Err(err) => Err(err.into()),
        }
    }
}
