pub mod bing;
pub mod manager;

use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::{Duration, Utc};
use log::{debug, error};
use tokio::sync::Mutex;
use zbus::{ConnectionBuilder, dbus_interface};
use tokio_walltime::sleep_until;
use crate::bing::Bing;
use crate::manager::{Configuration, LocalPicture, Manager, predict_next_poll_time};

async fn locate_bliss() -> Option<PathBuf> {
    let possibilities = [
        PathBuf::from("/usr/lib/bingwallpaper/bliss.jpg"),
        env::current_dir().unwrap().join("bliss.jpg"),
        env::current_exe().unwrap().join("bliss.jpg"),
    ];

    for possibility in possibilities {
        if let Ok(true) = tokio::fs::try_exists(&possibility).await {
            return Some(possibility);
        }
    }

    None
}

#[tokio::main]
async fn main() {
    env_logger::builder().target(env_logger::Target::Stdout).init();

    let bliss = locate_bliss().await.expect("locate default wallpaper");
    let bliss = bliss.to_string_lossy().to_string();
    let current_picture = Arc::new(Mutex::new(bliss));

    let bing = Bing::new();
    let configuration = Configuration::default();
    let manager = Manager::new(bing, configuration);

    // lock while looking for local pictures
    let mut picture = current_picture.lock().await;

    // start d-bus service as soon as possible
    let iface = BingWallpaper { current_wallpaper: current_picture.clone() };
    let connection = ConnectionBuilder::session().unwrap()
        .name("net.boothwhack.BingWallpaper1").unwrap()
        .serve_at("/net/boothwhack/BingWallpaper1", iface).unwrap()
        .build()
        .await.unwrap();

    // todo: fork process here?

    let mut wait_until = match manager.poll_local_picture().await {
        Some(LocalPicture::Today(path)) => {
            debug!("Located today's picture at {}", path.display());
            // today's picture is already available, all is good
            *picture = path.to_string_lossy().to_string();
            predict_next_poll_time()
        }
        Some(LocalPicture::Yesterday(path)) => {
            debug!("Located yesterday's picture at {}, refreshing in 1 minute", path.display());
            // yesterday's picture is available, use it and download today's in a minute to avoid
            // yesterday's picture appearing for only a split second
            *picture = path.to_string_lossy().to_string();
            Utc::now() + Duration::minutes(1)
        }
        // no local picture available, attempt to download one and fall back to bliss
        None => match manager.poll_picture().await {
            (Some(path), wait_until) => {
                debug!("Downloaded initial picture: {}", path.display());
                *picture = path.to_string_lossy().to_string();
                wait_until
            }
            (None, wait_until) => {
                debug!("Failed to download initial picture, falling back to bliss for now.");
                wait_until
            }
        },
    };

    // drop lock to allow dbus property to be read
    drop(picture);

    loop {
        debug!("Sleeping until {}", wait_until);
        if let Err(err) = sleep_until(wait_until).await {
            error!("Error while sleeping: {}", err);
        }

        let (path, next) = manager.poll_picture().await;
        wait_until = next;

        if let Some(path) = path {
            let mut picture = current_picture.lock().await;
            *picture = path.to_string_lossy().to_string();
            drop(picture);

            let iface_ref = connection.object_server().interface::<_, BingWallpaper>("/net/boothwhack/BingWallpaper1")
                .await.unwrap();
            let iface = iface_ref.get_mut().await;
            if let Err(err) = iface.current_wallpaper_changed(iface_ref.signal_context()).await {
                error!("Error while notifying property changed: {}", err);
            }
        }
    }
}

struct BingWallpaper {
    // todo: include metadata
    current_wallpaper: Arc<Mutex<String>>,
}

#[dbus_interface(name = "net.boothwhack.BingWallpaper1")]
impl BingWallpaper {
    #[dbus_interface(property)]
    async fn current_wallpaper(&self) -> String {
        let current_wallpaper = self.current_wallpaper.lock().await;
        current_wallpaper.clone()
    }
}
