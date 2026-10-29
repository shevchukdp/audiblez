//! HTTP client for the audiblez backend controller.
//!
//! The egui GUI is transport-agnostic: it talks JSON to *some* controller
//! (localhost when running locally, a spark01/spark02 box when in remote mode).
//! This module wraps the two endpoints we use: GET /status and POST /run.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct RunParams {
    pub file_path: String,
    pub voice: String,
    pub speed: f64,
    pub output_folder: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// base url of a remote_tts_server (Kokoro on a CUDA box). When set, the
    /// controller runs the pipeline but TTS inference goes to this server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_out: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_cover: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_manually: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_chapters: Option<i32>,
}

/// One file produced by a job in the server's output folder.
#[derive(Debug, Clone, Deserialize)]
pub struct OutputFile {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Status {
    pub status: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub stats: serde_json::Value,
    #[serde(default)]
    pub chapter: i64,
    #[serde(default)]
    pub chapters_total: i64,
}

#[derive(Debug, Clone)]
pub struct Client {
    base_url: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base_url: &str) -> Self {
        Client {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn set_url(&mut self, base_url: &str) {
        self.base_url = base_url.trim_end_matches('/').to_string();
    }

    pub async fn is_reachable(&self) -> bool {
        // A cheap GET that returns 404 for "/" but proves the TCP/HTTP path works.
        match self
            .http
            .get(format!("{}/status", self.base_url))
            .send()
            .await
        {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    pub async fn status(&self) -> Result<Status, String> {
        let url = format!("{}/status", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("controller status http {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn run(&self, params: &RunParams) -> Result<String, String> {
        let url = format!("{}/run", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(params)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            let status = resp.status();
            let msg = resp.text().await.unwrap_or_default();
            return Err(format!("controller returned {}: {}", status, msg));
        }
        Ok("job submitted".to_string())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let url = format!("{}/stop", self.base_url);
        self.http.post(&url).send().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Upload a book to the server (full-server mode). Returns the server-side
    /// path to use as `file_path` in run().
    pub async fn upload(&self, name: &str, bytes: Vec<u8>) -> Result<String, String> {
        let url = format!("{}/upload?name={}", self.base_url, pct_encode(name));
        let resp = self
            .http
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            let status = resp.status();
            let msg = resp.text().await.unwrap_or_default();
            return Err(format!("upload failed {}: {}", status, msg));
        }
        #[derive(Deserialize)]
        struct Up {
            path: String,
        }
        let up: Up = resp.json().await.map_err(|e| e.to_string())?;
        Ok(up.path)
    }

    /// List finished job outputs. Empty/"auto" folder = server default folder.
    pub async fn outputs(&self, folder: &str) -> Result<Vec<OutputFile>, String> {
        let url = format!("{}/outputs?folder={}", self.base_url, pct_encode(folder));
        let resp = self.http.get(&url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            let status = resp.status();
            let msg = resp.text().await.unwrap_or_default();
            return Err(format!("outputs failed {}: {}", status, msg));
        }
        #[derive(Deserialize)]
        struct Out {
            files: Vec<OutputFile>,
        }
        let out: Out = resp.json().await.map_err(|e| e.to_string())?;
        Ok(out.files)
    }

    /// Download one output file's bytes from the server.
    pub async fn download(&self, folder: &str, name: &str) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/download?folder={}&name={}",
            self.base_url,
            pct_encode(folder),
            pct_encode(name)
        );
        let resp = self.http.get(&url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            let status = resp.status();
            let msg = resp.text().await.unwrap_or_default();
            return Err(format!("download failed {}: {}", status, msg));
        }
        resp.bytes().await.map(|b| b.to_vec()).map_err(|e| e.to_string())
    }
}

/// Minimal percent-encoding for query parameters (leaves unreserved chars).
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
