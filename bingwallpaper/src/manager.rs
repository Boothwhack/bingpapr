use std::ops::{Add, Deref};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use chrono::{DateTime, Duration, Timelike, Utc};
use log::{debug, error, warn};
use crate::bing::{Bing, BING_DATE_FORMAT, Market};

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
            Some(base_dirs) => base_dirs.config_dir().join("bingwallpaper"),
            None => {
                PathBuf::from_str("~/.config/bingwallpaper").expect("Failed to get configuration directory")
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

pub struct Manager {
    bing: Bing,
    configuration: Configuration,
}

pub enum LocalPicture {
    Today(PathBuf),
    Yesterday(PathBuf),
}

pub fn predict_next_poll_time() -> DateTime<Utc> {
    let now = Utc::now();
    if now.hour() >= 7 {
        now.date_naive().add(Duration::days(1)).and_hms_opt(7, 0, 0).unwrap().and_utc()
    } else {
        now.date_naive().and_hms_opt(7, 0, 0).unwrap().and_utc()
    }
}

impl Manager {
    pub fn new(bing: Bing, configuration: Configuration) -> Self {
        Manager { bing, configuration }
    }

    pub async fn poll_local_picture(&self) -> Option<LocalPicture> {
        let today = Utc::now();
        let yesterday = today - Duration::hours(24);
        let today = today.format(BING_DATE_FORMAT).to_string();
        let yesterday = yesterday.format(BING_DATE_FORMAT).to_string();

        debug!("Looking for today's picture {} and yesterday's as fallback {}", today, yesterday);
        let mut yesterday_opt = None;

        let picture_directory = self.configuration.get_pictures_directory();
        let mut dir = tokio::fs::read_dir(picture_directory).await.ok()?;
        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&today) {
                return Some(LocalPicture::Today(entry.path()));
            } else if name.starts_with(&yesterday) {
                yesterday_opt = Some(entry.path());
            }
        }

        yesterday_opt.map(LocalPicture::Yesterday)
    }

    /// Attempts to downloads the image of the day from Bing and returns the time when the next
    /// poll operation should be performed.
    pub async fn poll_picture(&self) -> (Option<PathBuf>, DateTime<Utc>) {
        debug!("Polling picture");
        let image = match self.bing.image_of_the_day().await {
            Ok(image) => image,
            Err(error) => {
                error!("Failed to query image of the day: {}, retrying in an hour.", error);
                return (None, DateTime::from(Utc::now() + Duration::hours(1)));
            }
        };
        image.get_image_file_name();

        let picture_directory = self.configuration.get_pictures_directory();
        let picture_path = picture_directory.join(image.get_image_file_name());

        // check if picture is already downloaded
        if let Ok(true) = tokio::fs::try_exists(&picture_path).await {
            debug!("Picture already downloaded");
        } else {
            if let Err(error) = self.bing.download_image(&image, &picture_path).await {
                error!("Failed to download image: {}, retrying in an hour.", error);
                return (None, DateTime::from(Utc::now() + Duration::hours(1)));
            }
        }

        (Some(picture_path), match image.get_end_date() {
            Ok(end_date) if end_date < Utc::now() => {
                let next = predict_next_poll_time();
                warn!("Bing returned end date in the past, assuming {}", next);
                next
            }
            Ok(end_date) => end_date,
            Err(err) => {
                let next = predict_next_poll_time();
                warn!("Failed to parse end date: {}, assuming {}", err, next);
                next
            }
        })
    }
}
