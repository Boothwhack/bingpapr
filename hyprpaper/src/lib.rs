use std::time::Duration;
use std::{env, error, io};
use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use log::{debug, error};
use thiserror::Error;

pub struct Hyprpaper {
    pub socket_path: PathBuf,
}

pub type HyprpaperResult = Result<String, HyprpaperError>;

#[derive(Error, Debug)]
pub enum HyprpaperError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("unknown error from hyprpaper ipc")]
    Hyprpaper,
    #[error("image path contained invalid utf-8 characters")]
    InvalidPath,
}

fn path_to_string(path: &Path) -> HyprpaperResult {
    if let Some(path) = path.to_str() {
        Ok(path.to_string())
    } else {
        debug!("Could not convert '{}' to a string", path.display());
        Err(HyprpaperError::InvalidPath)
    }
}

impl Hyprpaper {
    pub fn new() -> Option<Hyprpaper> {
        let path = Path::new("/tmp/hypr");
        let socket_path = match env::var("HYPRLAND_INSTANCE_SIGNATURE") {
            Err(_) => path.join(".hyprpaper.sock"),
            Ok(sig) => path.join(sig).join(".hyprpaper.sock"),
        };
        Some(Hyprpaper { socket_path })
    }

    fn connect_to_socket(&self) -> Result<UnixStream, io::Error> {
        const ATTEMPTS: u32 = 5;
        for attempt in 1..=ATTEMPTS {
            debug!("Connecting to socket: {:?} attempt #{}", self.socket_path, attempt);
            match UnixStream::connect(&self.socket_path) {
                Ok(socket) => return Ok(socket),
                Err(err) => {
                    debug!("Error connecting: {:?}", err);
                    if attempt != ATTEMPTS {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                },
            }
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "Could not open hyprpaper socket"))
    }

    fn send(&self, msg: &str) -> HyprpaperResult {
        let mut socket = self.connect_to_socket()?;

        debug!("Sending message: {}", msg);
        socket.write(msg.as_bytes())?;

        let mut buf = [0u8; 2];
        let read = socket.read(&mut buf)?;
        socket.shutdown(Shutdown::Both)?;

        if read == 2 && buf[..2] == *b"ok" {
            Ok("ok".to_owned())
        } else {
            Err(HyprpaperError::Hyprpaper)
        }
    }

    pub fn preload(&self, path: &Path) -> HyprpaperResult {
        debug!("Preloading wallpaper: {}", path.display());
        let command = format!("preload {}\0", path_to_string(path)?);
        let output = self.send(&command)?;
        debug!("hyprpaper preload output: {}", output);
        Ok(output)
    }

    pub fn set_wallpaper(&self, monitor: &str, path: &Path) -> HyprpaperResult {
        debug!("Applying wallpaper '{}' to monitor: {}", path.display(), monitor);
        let command = format!("wallpaper {},{}", monitor, path_to_string(path)?);
        let output = self.send(&command)?;
        debug!("hyprpaper wallpaper output: {}", output);
        Ok(output)
    }

    pub fn unload(&self, path: &Path) -> HyprpaperResult {
        debug!("Unloading wallpaper: {}", path.display());
        let command = format!("unload {}", path_to_string(path)?);
        let output = self.send(&command)?;
        debug!("hyprpaper unload output: {}", output);
        Ok(output)
    }
}
