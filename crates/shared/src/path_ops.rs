//! Path operations (port of the pure `RichPath` half of `shared/PathOps.scala`).
//!
//! The `Log`-based `deleteDirectory`/`deleteSingleFile` are deferred.

use std::fs;
use std::io;
use std::path::Path;

/// Sum of all file sizes under `path` (port of `RichPath.folderSize`).
pub fn folder_size(path: &Path) -> io::Result<u64> {
    let mut total = 0u64;
    accumulate(path, &mut total)?;
    Ok(total)
}

fn accumulate(path: &Path, total: &mut u64) -> io::Result<()> {
    let meta = fs::metadata(path)?;
    if meta.is_file() {
        *total += meta.len();
    } else if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            accumulate(&entry?.path(), total)?;
        }
    }
    Ok(())
}

/// Recursively delete a directory tree (port of `RichPath.recursivelyDelete`).
pub fn recursively_delete(path: &Path) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rchain_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn folder_size_sums_file_sizes_recursively() {
        let dir = temp_dir("folder_size");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.txt"), b"hello").unwrap(); // 5 bytes
        fs::write(dir.join("sub/b.txt"), b"world!").unwrap(); // 6 bytes

        assert_eq!(folder_size(&dir).unwrap(), 11);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_size_of_a_file_is_its_length() {
        let dir = temp_dir("folder_size_file");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f.txt");
        fs::write(&file, b"abcdef").unwrap();

        assert_eq!(folder_size(&file).unwrap(), 6);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recursively_delete_removes_the_tree() {
        let dir = temp_dir("recursive_delete");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/f.txt"), b"x").unwrap();

        recursively_delete(&dir).unwrap();

        assert!(!dir.exists());
    }
}
