use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = temp_path_for(path);
    fs::write(&tmp_path, bytes)?;
    replace_file(&tmp_path, path)
}

pub(crate) fn write_text_atomic(path: &Path, text: &str) -> io::Result<()> {
    write_bytes_atomic(path, text.as_bytes())
}

fn replace_file(tmp_path: &Path, final_path: &Path) -> io::Result<()> {
    replace_file_with_installer(tmp_path, final_path, |from, to| fs::rename(from, to))
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("content.tmp");
    let tmp_name = format!("{file_name}.tmp");
    match path.parent() {
        Some(parent) => parent.join(tmp_name),
        None => PathBuf::from(tmp_name),
    }
}

fn backup_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("content.backup");
    let backup_name = format!("{file_name}.bak");
    match path.parent() {
        Some(parent) => parent.join(backup_name),
        None => PathBuf::from(backup_name),
    }
}

fn replace_file_with_installer<F>(
    tmp_path: &Path,
    final_path: &Path,
    mut install: F,
) -> io::Result<()>
where
    F: FnMut(&Path, &Path) -> io::Result<()>,
{
    let backup_path = backup_path_for(final_path);
    let mut moved_existing_to_backup = false;

    if final_path.exists() {
        match fs::remove_file(&backup_path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                let _ = fs::remove_file(tmp_path);
                return Err(error);
            }
        }

        if let Err(error) = fs::rename(final_path, &backup_path) {
            let _ = fs::remove_file(tmp_path);
            return Err(error);
        }
        moved_existing_to_backup = true;
    }

    match install(tmp_path, final_path) {
        Ok(()) => {
            if moved_existing_to_backup {
                match fs::remove_file(&backup_path) {
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        }
        Err(install_error) => {
            if moved_existing_to_backup {
                if let Err(rollback_error) = fs::rename(&backup_path, final_path) {
                    let _ = fs::remove_file(tmp_path);
                    return Err(io::Error::new(
                        install_error.kind(),
                        format!(
                            "failed to install temp file: {install_error}; rollback failed: {rollback_error}"
                        ),
                    ));
                }
            }
            let _ = fs::remove_file(tmp_path);
            Err(install_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use tempfile::TempDir;

    use super::{backup_path_for, replace_file_with_installer, temp_path_for, write_bytes_atomic};

    #[test]
    fn write_bytes_atomic_overwrite_replaces_and_cleans_backup() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("pack.bin");
        std::fs::write(&path, b"old").expect("seed old");

        write_bytes_atomic(&path, b"new").expect("atomic write");

        assert_eq!(std::fs::read(&path).expect("read final"), b"new");
        assert!(!backup_path_for(&path).exists());
    }

    #[test]
    fn replace_file_rolls_back_when_install_fails() {
        let dir = TempDir::new().expect("temp dir");
        let final_path = dir.path().join("manifest.json");
        let tmp_path = temp_path_for(&final_path);
        std::fs::write(&final_path, b"stable").expect("seed final");
        std::fs::write(&tmp_path, b"candidate").expect("seed tmp");

        let error = replace_file_with_installer(&tmp_path, &final_path, |_from, _to| {
            Err(io::Error::other("simulated install failure"))
        })
        .expect_err("install should fail");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            std::fs::read(&final_path).expect("final restored"),
            b"stable"
        );
        assert!(!tmp_path.exists());
        assert!(!backup_path_for(&final_path).exists());
    }

    #[test]
    fn replace_file_without_existing_destination_installs_directly() {
        let dir = TempDir::new().expect("temp dir");
        let final_path = dir.path().join("defs.pack");
        let tmp_path = temp_path_for(&final_path);
        std::fs::write(&tmp_path, b"bytes").expect("seed tmp");

        replace_file_with_installer(&tmp_path, &final_path, |from, to| std::fs::rename(from, to))
            .expect("install");

        assert_eq!(std::fs::read(&final_path).expect("final"), b"bytes");
        assert!(!backup_path_for(&final_path).exists());
    }
}
