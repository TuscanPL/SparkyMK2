//! A project's display images, in its `PICTURE` folder on the card.
//!
//! The device reads these when it loads the project, so a write shows up the next time
//! that project is selected (or at the next power-on, for the project that is current).

use crate::{Device, Error, Result};

/// Frames of the animation shown while a project loads.
pub const STARTUP_SLOTS: [&str; 2] = ["startup_1", "startup_2"];
/// Frames of the screen saver.
pub const SCREEN_SAVER_SLOTS: [&str; 4] = [
    "screen_saver_1",
    "screen_saver_2",
    "screen_saver_3",
    "screen_saver_4",
];

/// Every slot a project has, startup frames first.
pub fn slots() -> impl Iterator<Item = &'static str> {
    STARTUP_SLOTS.into_iter().chain(SCREEN_SAVER_SLOTS)
}

/// Path of a slot's BMP on the card. `project` is 1-based.
pub fn screen_path(project: u8, slot: &str) -> Result<String> {
    if !slots().any(|s| s == slot) {
        return Err(Error::Refused(format!("unknown display image {slot:?}")));
    }
    Ok(format!(
        "ROLAND/SP-404MKII/PROJECT_{project:02}/PICTURE/{slot}.bmp"
    ))
}

impl Device {
    /// Read a display image as it is stored; `None` when the project has no such file.
    pub fn read_screen(&self, project: u8, slot: &str) -> Result<Option<Vec<u8>>> {
        let path = screen_path(project, slot)?;
        match self.stat(&path)? {
            Some(_) => self.read_file(&path).map(Some),
            None => Ok(None),
        }
    }

    /// Replace a display image. `bmp` must already be in the card's format.
    pub fn write_screen(&self, project: u8, slot: &str, bmp: &[u8]) -> Result<()> {
        self.write_file(&screen_path(project, slot)?, bmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_paths_for_known_slots_only() {
        assert_eq!(
            screen_path(6, "startup_2").unwrap(),
            "ROLAND/SP-404MKII/PROJECT_06/PICTURE/startup_2.bmp"
        );
        assert_eq!(slots().count(), 6);
        assert!(screen_path(6, "../PADCONF.BIN").is_err());
        assert!(screen_path(6, "startup_3").is_err());
    }
}
