use std::fs::File;
use std::io;

pub(crate) fn copy(source: &mut File, destination: &mut File) -> io::Result<u64> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: both descriptors are live files; FICLONE takes the source descriptor as an integer.
        if unsafe { libc::ioctl(destination.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) } == 0 {
            return source.metadata().map(|metadata| metadata.len());
        }
        let error = io::Error::last_os_error();
        if !matches!(
            error.raw_os_error(),
            Some(
                libc::EXDEV
                    | libc::EOPNOTSUPP
                    | libc::ENOTTY
                    | libc::EINVAL
                    | libc::ENOSYS
                    | libc::EPERM
            )
        ) {
            return Err(error);
        }
    }
    io::copy(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    fn copied_file_keeps_independent_contents_after_source_changes() {
        let root = std::env::temp_dir().join(format!("legio-copy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let original = root.join("source");
        let copied = root.join("copy");
        std::fs::write(&original, b"original").unwrap();
        let mut source = File::open(&original).unwrap();
        let mut destination = File::options()
            .write(true)
            .create_new(true)
            .open(&copied)
            .unwrap();
        assert_eq!(copy(&mut source, &mut destination).unwrap(), 8);
        destination.seek(SeekFrom::Start(0)).unwrap();
        destination.write_all(b"new data").unwrap();
        assert_eq!(std::fs::read(original).unwrap(), b"original");
        std::fs::write(&copied, b"changed").unwrap();
        assert_eq!(std::fs::read(copied).unwrap(), b"changed");
        drop(source);
        drop(destination);
        std::fs::remove_dir_all(root).unwrap();
    }
}
