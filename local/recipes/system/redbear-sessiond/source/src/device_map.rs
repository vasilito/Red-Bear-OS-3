use std::{collections::HashMap, fs::File, io};

#[derive(Clone, Debug)]
pub struct DeviceMap {
    static_paths: HashMap<(u32, u32), String>,
}

impl DeviceMap {
    pub fn new() -> Self {
        let static_paths = HashMap::from([
            ((226, 0), String::from("/scheme/drm/card0")),
            ((226, 1), String::from("/scheme/drm/card1")),
            ((13, 64), String::from("/dev/input/event0")),
            ((13, 65), String::from("/dev/input/event1")),
            ((13, 66), String::from("/dev/input/event2")),
            ((13, 67), String::from("/dev/input/event3")),
            ((29, 0), String::from("/dev/fb0")),
            ((1, 1), String::from("/scheme/null")),
            ((1, 5), String::from("/scheme/zero")),
            ((1, 8), String::from("/scheme/rand")),
        ]);

        Self { static_paths }
    }

    pub fn resolve(&self, major: u32, minor: u32) -> Option<String> {
        if let Some(path) = self.static_paths.get(&(major, minor)) {
            return Some(path.clone());
        }

        match (major, minor) {
            (13, minor) if minor >= 68 => Some(format!("/dev/input/event{}", minor - 64)),
            _ => None,
        }
    }

    pub fn open_device(&self, major: u32, minor: u32) -> io::Result<File> {
        let Some(path) = self.resolve(major, minor) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no Red Bear device mapping for major={major}, minor={minor}"),
            ));
        };

        File::open(path)
    }
}
