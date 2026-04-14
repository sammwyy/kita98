use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Cursor};
use anyhow::Result;

pub enum FileHandle {
    Real(File),
    Virtual(Cursor<Vec<u8>>),
}

impl Read for FileHandle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            FileHandle::Real(f) => f.read(buf),
            FileHandle::Virtual(c) => c.read(buf),
        }
    }
}

impl Seek for FileHandle {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            FileHandle::Real(f) => f.seek(pos),
            FileHandle::Virtual(c) => c.seek(pos),
        }
    }
}

pub struct DosRuntime {
    pub files: HashMap<u16, FileHandle>,
    pub next_handle: u16,
}

impl DosRuntime {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            next_handle: 5,
        }
    }

    pub fn open(&mut self, path: &str) -> Result<u16> {
        let file = File::open(path)?;
        let handle = self.next_handle;
        self.files.insert(handle, FileHandle::Real(file));
        self.next_handle += 1;
        Ok(handle)
    }

    pub fn read(&mut self, handle: u16, buf: &mut [u8]) -> Result<usize> {
        if let Some(file) = self.files.get_mut(&handle) {
            Ok(file.read(buf)?)
        } else {
            anyhow::bail!("Invalid handle {}", handle)
        }
    }

    pub fn seek(&mut self, handle: u16, offset: i64, whence: u8) -> Result<u64> {
        if let Some(file) = self.files.get_mut(&handle) {
            let pos = match whence {
                0 => SeekFrom::Start(offset as u64),
                1 => SeekFrom::Current(offset),
                2 => SeekFrom::End(offset),
                _ => anyhow::bail!("Invalid whence"),
            };
            Ok(file.seek(pos)?)
        } else {
            anyhow::bail!("Invalid handle {}", handle)
        }
    }

    pub fn close(&mut self, handle: u16) {
        self.files.remove(&handle);
    }
}
