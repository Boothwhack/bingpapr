mod bing;
mod hyprpaper;

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
use crate::bing::{Bing, Market};
use crate::hyprpaper::Hyprpaper;

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

#[derive(Debug, Default)]
pub struct Configuration {
    pub market: Market,
    /// Alternative directory to store downloaded wallpaper files. Defaults to
    /// '$XDG_PICTURES_DIR/Bing Wallpapers' if available, otherwise the configuration directory.
    pub pictures_directory: Option<String>,
}

impl Configuration {
    fn get_config_directory() -> PathBuf {
        match directories::BaseDirs::new() {
            Some(base_dirs) => base_dirs.config_dir().join("hypr"),
            None => {
                PathBuf::from_str("~/.config/hypr").expect("Failed to get configuration directory")
            }
        }
    }

    fn get_pictures_directory(&self) -> PathBuf {
        if let Some(pictures_directory) = self.pictures_directory.as_ref() {
            return PathBuf::from(pictures_directory);
        }

        if let Some(user_dirs) = directories::UserDirs::new() {
            if let Some(pictures_dir) = user_dirs.picture_dir() {
                return pictures_dir.join("Bing Wallpapers");
            }
        }
        Self::get_config_directory().join("bing-wallpaper-cache")
    }
}

struct BingWallpaper {
    bing: Bing,
    hyprpaper: Hyprpaper,
    configuration: Configuration,
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
    async fn update_wallpaper(&self) -> DateTime<Utc> {
        let mut last_picture = self.last_picture.lock().unwrap();

        debug!("Updating wallpaper");
        let image = match self.bing.image_of_the_day().await {
            Ok(image) => image,
            Err(error) => {
                error!("Failed to query image of the day: {}, retrying in an hour.", error);
                return DateTime::from(Utc::now() + Duration::hours(1));
            }
        };
        image.get_image_file_name();

        let picture_directory = self.configuration.get_pictures_directory();
        let picture_path = picture_directory.join(image.get_image_file_name());

        if last_picture.as_ref().map_or(false, |last_picture| &picture_path == last_picture) {
            debug!("Bing returned same picture as last time");
        } else {
            // check if picture is already downloaded
            if !picture_path.exists() {
                if let Err(error) = self.bing.download_image(&image, &picture_path).await {
                    error!("Failed to download image: {}, retrying in an hour.", error);
                    return DateTime::from(Utc::now() + Duration::hours(1));
                }
            } else {
                debug!("Picture already downloaded");
            }

            debug!("Preloading wallpaper");
            if let Err(error) = self.hyprpaper.preload(&picture_path) {
                error!("Failed to preload wallpaper: {}, retrying in an hour", error);
                return DateTime::from(Utc::now() + Duration::hours(1));
            }

            debug!("Applying wallpaper");
            if let Err(error) = self.apply_wallpaper_to_all_monitors(&picture_path).await {
                error!("Failed to apply wallpaper: {}", error);
            }

            if let Some(last_picture) = last_picture.as_ref() {
                if last_picture != &picture_path {
                    debug!("Unloading old wallpaper: {}", last_picture.display());
                    if let Err(error) = self.hyprpaper.unload(last_picture) {
                        error!("Failed to unload old wallpaper: {}", error);
                    }
                }
            }

            *last_picture = Some(picture_path);
        }

        match image.get_end_date() {
            Ok(end_date) if end_date < Utc::now() => {
                warn!("Bing returned end date in the past, assuming 24 hours from now");
                Utc::now() + Duration::hours(24)
            }
            Ok(end_date) => end_date,
            Err(err) => {
                warn!("Failed to parse end date: {}, assuming 24 hours from now", err);
                Utc::now() + Duration::hours(24)
            }
        }
    }

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

    let bing = Bing::new();
    let configuration = Configuration::default();
    let hyprpaper = Hyprpaper::new().expect("failed to connect to hyprpaper IPC");

    let local = LocalSet::new();

    // TODO: Ensure copyright tag is properly set on image file

    let bing = BingWallpaper {
        bing,
        hyprpaper,
        configuration,
        last_picture: Mutex::new(None),
    };
    let bing = Arc::new(bing);

    {
        let bing = bing.clone();
        local.spawn_local(async move {
            let mut next = bing.update_wallpaper().await;
            loop {
                debug!("Sleeping until {}", next);
                if let Err(err) = sleep_until(next).await {
                    error!("Error while sleeping: {}", err);
                }
                next = bing.update_wallpaper().await;
            }
        });
    }

    {
        let bing = bing.clone();
        let mut listener = EventListener::new();
        listener.add_monitor_added_handler(move |monitor| {
            debug!("Monitor added: {}", monitor);
            bing.on_monitor_added(&monitor);
        });
        local.spawn_local(async move {
            if let Err(err) = listener.start_listener_async().await {
                error!("Failed to start event listener: {}", err);
                process::exit(1)
            }
        });
    }

    local.await;
}
