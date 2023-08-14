use std::fmt::{Debug, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use log::debug;
use serde::Deserialize;
use thiserror::Error;
use tokio::fs::{create_dir_all, File};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

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

const BING_IMAGE_API_BASE_URL: &str = "https://www.bing.com/HPImageArchive.aspx";
const BING_BASE_URL: &str = "https://www.bing.com";

#[derive(Deserialize)]
struct BingAPIResponse {
    images: Vec<BingImage>,
}

#[derive(Deserialize)]
pub struct BingImage {
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

pub const BING_DATE_FORMAT: &str = "%Y%m%d";
pub const TIME_FORMAT: &str = "%H%M";

pub fn parse_bing_date(s: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    let (date, time) = NaiveDate::parse_and_remainder(s, &BING_DATE_FORMAT)?;
    let time = NaiveTime::parse_from_str(time, &TIME_FORMAT)
        .unwrap_or_else(|_| NaiveTime::from_hms_opt(7, 0, 0).unwrap());

    Ok(date.and_time(time).and_utc())
}

impl BingImage {
    pub fn get_image_url(&self) -> String {
        format!("{}{}_UHD.jpg", BING_BASE_URL, self.url_base)
    }

    pub fn get_image_file_name(&self) -> String {
        format!("{}-{}.jpg", self.start_date, self.title)
    }

    pub fn get_end_date(&self) -> Result<DateTime<Utc>, chrono::ParseError> {
        parse_bing_date(&self.end_date)
    }
}

#[derive(Debug, Error)]
pub enum ImageOfTheDayError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Bing API did not return any images")]
    NoImagesFound,
}

#[derive(Debug, Error)]
pub enum DownloadImageError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Failed to write image to {0:?}: {1}")]
    IoError(PathBuf, #[source] io::Error),
}

pub struct Bing {
    client: reqwest::Client,
}

impl Bing {
    pub fn new() -> Bing {
        Bing {
            client: reqwest::Client::new(),
        }
    }

    pub async fn image_of_the_day(&self) -> Result<BingImage, ImageOfTheDayError> {
        let mut response = self
            .client
            .get(BING_IMAGE_API_BASE_URL)
            .query(&[
                ("format", "js"),
                ("idx", "0"),
                ("n", "1"),
            ])
            .send()
            .await?
            .json::<BingAPIResponse>()
            .await?;

        let mut images = response.images.drain(..);
        images.next().ok_or(ImageOfTheDayError::NoImagesFound)
    }

    pub async fn download_image(&self, image: &BingImage, path: &Path) -> Result<(), DownloadImageError> {
        let url = image.get_image_url();

        debug!("Downloading image from {} into {}", url, path.display());

        let response = self.client.get(&url).send().await?;
        if let Some(parent) = path.parent() {
            if let Ok(false) = tokio::fs::try_exists(parent).await {
                create_dir_all(parent).await
                    .map_err(|err| DownloadImageError::IoError(path.to_path_buf(), err))?;
            }
        }
        let mut file = File::create(&path)
            .await
            .map_err(|err| DownloadImageError::IoError(path.to_owned(), err))?;
        let mut bytes = response.bytes_stream();
        while let Some(Ok(item)) = bytes.next().await {
            file.write_all(&item).await.map_err(|err| DownloadImageError::IoError(path.to_owned(), err))?;
        }
        Ok(())
    }
}
