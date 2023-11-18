use std::fmt::Debug;
use std::io;
use std::mem::swap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use hyprland::event_listener::EventListener;
use hyprland::prelude::*;
use log::{error, warn};
use thiserror::Error;
use tokio::{join, spawn};
use tokio::sync::Mutex;
use zbus::Connection;
use zbus::export::futures_util::StreamExt;

use hyprpaper::Hyprpaper;

mod bingdaily;

#[derive(Debug, Error)]
enum ApplyWallpaperError {
    #[error(transparent)]
    HyprError(#[from] hyprland::shared::HyprError),
    #[error(transparent)]
    HyprpaperError(#[from] hyprpaper::HyprpaperError),
    #[error(transparent)]
    IoError(#[from] io::Error),
}

struct BingPapr {
    hyprpaper: Hyprpaper,
    active_picture: PathBuf,
}

impl BingPapr {
    async fn set_new_wallpaper(&mut self, path: impl Into<PathBuf>) -> Result<(), ApplyWallpaperError> {
        let mut old_picture = path.into();
        swap(&mut old_picture, &mut self.active_picture);

        // apply new wallpaper before unloading the old one
        self.hyprpaper.preload(&self.active_picture)?;
        if let Err(error) = self.apply_wallpaper_to_all_monitors(&self.active_picture).await {
            warn!("Failed to apply wallpaper '{}' to all monitors: {}", self.active_picture.display(), error);
        }
        self.hyprpaper.unload(&old_picture)?;

        Ok(())
    }

    fn on_monitor_added(&self, monitor: &str) {
        if let Err(err) = self.apply_wallpaper_to_monitor(&monitor, &self.active_picture) {
            error!("Failed to apply wallpaper to monitor: {}", err);
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

    let connection = Connection::session().await.expect("dbus session");
    let bingwallpaper = bingdaily::BingDaily1Proxy::new(&connection).await.expect("BingWallpaper proxy");

    let hyprpaper = Hyprpaper::new().expect("failed to connect to hyprpaper IPC");

    // get initial wallpaper
    let path = bingwallpaper.current_picture().await.expect("wallpaper property");
    let path = PathBuf::from_str(&path).expect("wallpaper path");

    let bingpaper = Arc::new(Mutex::new(BingPapr {
        active_picture: path.clone(),
        hyprpaper,
    }));

    // apply initial wallpaper
    {
        let bingpaper = bingpaper.lock().await;
        bingpaper.hyprpaper.preload(&path).expect("preload wallpaper");
        if let Err(error) = bingpaper.apply_wallpaper_to_all_monitors(&path).await {
            warn!("Failed to apply wallpaper '{}' to all monitors: {}", path.display(), error)
        }
    }

    let watch_property_task = {
        let bingpaper = bingpaper.clone();
        spawn(async move {
            while let Some(wallpaper) = bingwallpaper.receive_current_picture_changed().await.next().await {
                let wallpaper = wallpaper.get().await.expect("wallpaper property");
                let path = PathBuf::from_str(&wallpaper).expect("wallpaper path");

                let mut bingpaper = bingpaper.lock().await;
                if let Err(error) = bingpaper.set_new_wallpaper(&path).await {
                    warn!("Failed to set new wallpaper '{}': {}", path.display(), error);
                }
            }
        })
    };

    let watch_monitors_task = {
        let bingpaper = bingpaper.clone();
        spawn(async move {
            let mut event_listener = EventListener::new();
            event_listener.add_monitor_added_handler(move |monitor| {
                let bingpaper = bingpaper.clone();
                spawn(async move {
                    let bingpaper = bingpaper.lock().await;
                    bingpaper.on_monitor_added(&monitor);
                });
            });

            event_listener.start_listener_async().await
                .expect("failed to start event listener");
        })
    };

    join!(watch_property_task, watch_monitors_task);
}
