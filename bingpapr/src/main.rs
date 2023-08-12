use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::{io, process};
use std::str::FromStr;
use std::sync::Arc;
use chrono::{DateTime, Duration, Utc};
use hyprland::event_listener::EventListener;
use hyprland::prelude::*;
use log::{debug, error, warn};
use thiserror::Error;
use std::sync::Mutex;
use tokio::task::LocalSet;
use tokio_walltime::sleep_until;
use hyprpaper::Hyprpaper;

#[derive(Debug, Error)]
enum ApplyWallpaperError {
    #[error(transparent)]
    HyprError(#[from] hyprland::shared::HyprError),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error("hyprctl exited with code {0:?}: {1}")]
    ExecuteHyprCtlError(Option<i32>, String),
    #[error("hyprctl exited with code {0:?} and parsing output as UTF-8 failed: {1}")]
    ParseUtf8Error(Option<i32>, #[source] std::string::FromUtf8Error),
}

struct BingWallpaper {
    hyprpaper: Hyprpaper,
    last_picture: Mutex<Option<PathBuf>>,
}

impl BingWallpaper {
    fn on_monitor_added(&self, monitor: &str) {
        let last_picture = self.last_picture.lock().unwrap();

        if let Some(last_picture) = last_picture.as_ref() {
            if let Err(err) = self.apply_wallpaper_to_monitor(&monitor, last_picture) {
                error!("Failed to apply wallpaper to monitor: {}", err);
            }
        }
    }

    /// Applies the current wallpaper from Bing to all monitors. Returns the time when the
    /// wallpaper should be updated next.


    async fn apply_wallpaper_to_all_monitors(&self, path: &Path) -> Result<(), ApplyWallpaperError> {
        let monitors = hyprland::data::Monitors::get_async().await?;

        for monitor in monitors {
            self.apply_wallpaper_to_monitor(&monitor.name, path)?;
        }

        Ok(())
    }

    fn apply_wallpaper_to_monitor(&self, monitor: &str, path: &Path) -> Result<(), ApplyWallpaperError> {
        self.hyprpaper.set_wallpaper(monitor, path)?;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    env_logger::builder().target(env_logger::Target::Stdout).init();

    let hyprpaper = Hyprpaper::new().expect("failed to connect to hyprpaper IPC");

    // TODO: Ensure copyright tag is properly set on image file

    let bing = BingWallpaper {
        hyprpaper,
        last_picture: Mutex::new(None),
    };

    todo!()
}
