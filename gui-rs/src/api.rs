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

    pub fn is_reachable(&self) -> bool {
        // A cheap GET that returns 404 for "/" but proves the TCP/HTTP path works.
        pollster::block_on(async {
            match self
                .http
                .get(format!("{}/status", self.base_url))
                .send()
                .await
            {
                Ok(r) => r.status().is_success(),
                Err(_) => false,
            }
        })
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
}
