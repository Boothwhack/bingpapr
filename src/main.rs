use std::fmt::{Debug, format, Formatter, Write};
use std::path::{Path, PathBuf};
use std::{io, process};
use std::process::{ExitStatus, Output};
use std::str::FromStr;
use std::sync::Arc;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use hyprland::command;
use hyprland::event_listener::{AsyncEventListener, EventListener};
use hyprland::prelude::*;
use little_exif::metadata::Metadata;
use log::{debug, error};
use reqwest::{Client, Error, Response};
use serde::Deserialize;
use thiserror::Error;
use tokio::fs::{create_dir_all, File};
use tokio::io::AsyncWriteExt;
use std::sync::Mutex;
use hyprland::data::Monitor;
use tokio::task::{LocalSet, spawn_local};
use tokio_stream::StreamExt;
use tokio_walltime::sleep_until;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_registry::{WlRegistry};

#[derive(Default)]
pub enum Market {
    DanishDenmark,
    EnglishGB,
    #[default]
    EnglishUS,
}

impl Debug for Market {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string())
    }
}

impl ToString for Market {
    fn to_string(&self) -> String {
        match self {
            Market::DanishDenmark => "da-DK".to_owned(),
            Market::EnglishGB => "en-GB".to_owned(),
            Market::EnglishUS => "en-US".to_owned(),
        }
    }
}

#[derive(Debug, Error)]
#[error("Unknown market: {0}")]
pub struct UnknownMarket(String);

impl FromStr for Market {
    type Err = UnknownMarket;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "da-DK" => Ok(Market::DanishDenmark),
            "en-GB" => Ok(Market::EnglishGB),
            "en-US" => Ok(Market::EnglishUS),
            _ => Err(UnknownMarket(s.to_owned())),
        }
    }
}

#[derive(Debug, Default)]
struct Configuration {
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

const BING_IMAGE_API_BASE_URL: &str = "https://www.bing.com/HPImageArchive.aspx";
const BING_BASE_URL: &str = "https://www.bing.com";

#[derive(Deserialize)]
struct BingAPIResponse {
    images: Vec<BingImage>,
}

#[derive(Deserialize)]
struct BingImage {
    #[serde(rename = "startdate")]
    start_date: String,
    #[serde(rename = "fullstartdate")]
    full_start_date: String,
    #[serde(rename = "enddate")]
    end_date: String,
    url: String,
    #[serde(rename = "urlbase")]
    url_base: String,
    title: String,
}

impl BingImage {
    fn get_image_url(&self) -> String {
        format!("{}{}_UHD.jpg", BING_BASE_URL, self.url_base)
    }

    fn get_image_file_name(&self) -> String {
        format!("{}-{}.jpg", self.start_date, self.title)
    }
}

#[derive(Debug, Error)]
enum QueryImageOfTheDayError {
    #[error(transparent)]
    RequestError(#[from] Error),
    #[error("Bing API did not return any images")]
    NoImagesFound,
}

async fn query_image_of_the_day(client: &Client) -> Result<BingImage, QueryImageOfTheDayError> {
    let response = client.get(BING_IMAGE_API_BASE_URL)
        .query(&[
            ("format", "js"),
            ("idx", "0"),
            ("n", "1")
        ])
        .send().await?;
    let mut response = response.json::<BingAPIResponse>().await?;

    let mut images = response.images.drain(..);
    images.next().ok_or(QueryImageOfTheDayError::NoImagesFound)
}

#[derive(Debug, Error)]
enum DownloadImageError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Failed to write image to {0:?}: {1}")]
    IoError(PathBuf, #[source] io::Error),
}

async fn download_image(image: &BingImage, path: &Path) -> Result<(), DownloadImageError> {
    let url = image.get_image_url();
    debug!("Downloading image from {} into {}", url, path.display());

    let response = reqwest::get(&url).await?;
    let mut file = File::create(path).await.map_err(|err| DownloadImageError::IoError(path.to_owned(), err))?;
    let mut bytes = response.bytes_stream();
    while let Some(Ok(item)) = bytes.next().await {
        file.write_all(&item).await.map_err(|err| DownloadImageError::IoError(path.to_owned(), err))?;
    }
    Ok(())
}

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

fn execute_hyprctl_hyprpaper(command: &str, argument: &str) -> Result<(), ApplyWallpaperError> {
    debug!("Executing: hyprctl hyprpaper {} {}", command, argument);
    let output = process::Command::new("hyprctl")
        .arg("hyprpaper")
        .arg(command)
        .arg(argument)
        .output()?;
    if !output.status.success() {
        return Err(ApplyWallpaperError::ExecuteHyprCtlError(
            output.status.code(),
            String::from_utf8(output.stderr)
                .map_err(|err| ApplyWallpaperError::ParseUtf8Error(output.status.code(), err))?,
        ));
    }

    if let Ok(stdout) = String::from_utf8(output.stdout) {
        debug!("Output stdout:\n{}", stdout);
    }
    if let Ok(stderr) = String::from_utf8(output.stderr) {
        debug!("Output stderr:\n{}", stderr);
    }

    Ok(())
}

async fn apply_wallpaper_to_all_monitors(path: &Path) -> Result<(), ApplyWallpaperError> {
    debug!("Preloading wallpaper: {}", path.display());

    execute_hyprctl_hyprpaper("preload", &path.display().to_string())?;

    let monitors = hyprland::data::Monitors::get_async().await?;

    for monitor in monitors {
        apply_wallpaper_to_monitor(path, &monitor.name)?;
    }

    Ok(())
}

fn apply_wallpaper_to_monitor(path: &Path, monitor: &str) -> Result<(), ApplyWallpaperError> {
    let path = path.display();
    debug!("Applying wallpaper to monitor: {}", path);

    let wallpaper_argument = format!("{},{}", monitor, path);
    execute_hyprctl_hyprpaper("wallpaper", &wallpaper_argument)?;

    Ok(())
}

struct ParseBingDateError;

fn parse_date(date: &str) -> Result<DateTime<Utc>, ParseBingDateError> {
    let chars = date.chars();

    let year = chars.clone().take(4).collect::<String>().parse::<i32>().map_err(|_| ParseBingDateError)?;
    let month = chars.clone().skip(4).take(2).collect::<String>().parse::<u32>().map_err(|_| ParseBingDateError)?;
    let day = chars.clone().skip(6).take(2).collect::<String>().parse::<u32>().map_err(|_| ParseBingDateError)?;

    Ok(NaiveDate::from_ymd_opt(year, month, day).ok_or(ParseBingDateError)?
        .and_hms_opt(0, 0, 0).ok_or(ParseBingDateError)?
        .and_utc())
}

struct BingWallpaper {
    client: Client,
    configuration: Configuration,
    last_picture: Mutex<Option<PathBuf>>,
}

impl BingWallpaper {
    fn on_monitor_added(&self, monitor: &str) {
        let last_picture = self.last_picture.lock().unwrap();

        if let Some(last_picture) = last_picture.as_ref() {
            if let Err(err) = apply_wallpaper_to_monitor(last_picture, &monitor) {
                error!("Failed to apply wallpaper to monitor: {}", err);
            }
        }
    }

    /// Applies the current wallpaper from Bing to all monitors. Returns the time when the
    /// wallpaper should be updated next.
    async fn update_wallpaper(&self) -> DateTime<Utc> {
        let mut last_picture = self.last_picture.lock().unwrap();

        debug!("Updating wallpaper");
        let image = match query_image_of_the_day(&self.client).await {
            Ok(image) => image,
            Err(error) => {
                error!("Failed to query image of the day: {}, retrying in an hour.", error);
                return DateTime::from(Utc::now() + Duration::hours(1));
            }
        };
        image.get_image_file_name();

        // check if picture is already downloaded
        let picture_directory = self.configuration.get_pictures_directory();
        let picture_path = picture_directory.join(image.get_image_file_name());

        if !picture_path.exists() {
            if let Err(error) = download_image(&image, &picture_path).await {
                error!("Failed to download image: {}, retrying in an hour.", error);
                return DateTime::from(Utc::now() + Duration::hours(1));
            }
        } else {
            debug!("Picture already downloaded");
        }

        debug!("Applying wallpaper");
        if let Err(error) = apply_wallpaper_to_all_monitors(&picture_path).await {
            error!("Failed to apply wallpaper: {}", error);
        }

        if let Some(last_picture) = last_picture.as_ref() {
            if last_picture != &picture_path {
                debug!("Unloading old wallpaper: {}", last_picture.display());
                if let Err(error) = execute_hyprctl_hyprpaper("unload", &last_picture.display().to_string()) {
                    error!("Failed to unload old wallpaper: {}", error);
                }
            }
        }

        *last_picture = Some(picture_path);

        parse_date(&image.end_date).unwrap_or_else(|_| Utc::now() + Duration::hours(24))
    }
}

#[tokio::main]
async fn main() {
    env_logger::builder().target(env_logger::Target::Stdout).init();

    let client = Client::new();
    let configuration = Configuration::default();

    let local = LocalSet::new();

    // TODO: Ensure copyright tag is properly set on image file

    let bing = BingWallpaper {
        client,
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
