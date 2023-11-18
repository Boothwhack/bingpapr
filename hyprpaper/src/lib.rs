use std::{env, io};
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
}

fn path_to_string(path: &Path) -> HyprpaperResult {
    if let Some(path) = path.to_str() {
        Ok(path.to_string())
    } else {
        Err(io::Error::new(ErrorKind::NotFound, "Path not found").into())
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

    fn send(&self, msg: &[u8]) -> HyprpaperResult {
        let mut socket = UnixStream::connect(&self.socket_path)?;
        let mut buf = [0u8; 2];

        socket.write(msg)?;
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
        let output = self.send(command.as_bytes())?;
        debug!("hyprpaper preload output: {}", output);
        Ok(output)
    }

    pub fn set_wallpaper(&self, monitor: &str, path: &Path) -> HyprpaperResult {
        debug!("Applying wallpaper '{}' to monitor: {}", path.display(), monitor);
        let command = format!("wallpaper {},{}", monitor, path_to_string(path)?);
        let output = self.send(command.as_bytes())?;
        debug!("hyprpaper wallpaper output: {}", output);
        Ok(output)
    }

    pub fn unload(&self, path: &Path) -> HyprpaperResult {
        debug!("Unloading wallpaper: {}", path.display());
        let command = format!("unload {}", path_to_string(path)?);
        let output = self.send(command.as_bytes())?;
        debug!("hyprpaper unload output: {}", output);
        Ok(output)
    }
}
