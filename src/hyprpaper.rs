use std::{env, io};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use log::debug;

pub struct Hyprpaper {
    pub socket_path: PathBuf,
}

pub type HyprpaperResult = Result<String, io::Error>;

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

        socket.write_all(msg)?;
        let mut output = String::new();
        socket.read_to_string(&mut output)?;
        Ok(output)
    }

    pub fn preload(&self, path: &Path) -> HyprpaperResult {
        debug!("Preloading wallpaper: {}", path.display());
        let path = if let Some(path) = path.to_str() {
            path
        } else {
            return Err(io::Error::new(ErrorKind::NotFound, "Path not found"));
        };
        let command = format!("preload {}", path);
        let output = self.send(command.as_bytes())?;
        debug!("hyprpaper preload output: {}", output);
        Ok(output)
    }

    pub fn set_wallpaper(&self, monitor: &str, path: &Path) -> HyprpaperResult {
        debug!("Applying wallpaper to monitor: {}", path.display());
        let path = if let Some(path) = path.to_str() {
            path
        } else {
            return Err(io::Error::new(ErrorKind::NotFound, "Path not found"));
        };
        let command = format!("wallpaper {},{}", monitor, path);
        let output = self.send(command.as_bytes())?;
        debug!("hyprpaper wallpaper output: {}", output);
        Ok(output)
    }

    pub fn unload(&self, path: &Path) -> HyprpaperResult {
        debug!("Unloading wallpaper: {}", path.display());
        let path = if let Some(path) = path.to_str() {
            path
        } else {
            return Err(io::Error::new(ErrorKind::NotFound, "Path not found"));
        };
        let command = format!("unload {}", path);
        let output = self.send(command.as_bytes())?;
        debug!("hyprpaper unload output: {}", output);
        Ok(output)
    }
}
