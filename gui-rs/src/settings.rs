//! Persistent application settings (loaded from a small JSON file next to the
//! binary on Windows, so the user's model path / remote URL / options survive
//  restarts). Kept separate from the egui App so it is easy to test.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// true = talk to a remote controller, false = local controller.
    pub remote: bool,
    /// base url of the controller, e.g. http://spark01:8765 or http://127.0.0.1:8765
    pub controller_url: String,
    /// base url of a remote_tts_server (Kokoro on CUDA), e.g.
    /// http://100.66.167.48:8000. Blank = synthesize locally on the controller.
    #[serde(default)]
    pub tts_url: String,
    pub default_voice: String,
    pub bitrate: String,
    pub quality: i32,
    pub cover: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            remote: false,
            controller_url: "http://127.0.0.1:8765".into(),
            tts_url: String::new(),
            default_voice: "af_sky".into(),
            bitrate: "128".into(),
            quality: 4,
            cover: true,
        }
    }
}

impl Settings {
    /// Path of the settings file: on Windows it lives next to the binary
    /// (so a portable exe keeps its config beside the executable).
    pub fn path() -> Result<PathBuf, String> {
        let exe = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_extension("json");
        Ok(exe)
    }

    pub fn load() -> Self {
        let path = match Self::path() {
            Ok(p) => p,
            Err(_) => return Self::default(),
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }
}
