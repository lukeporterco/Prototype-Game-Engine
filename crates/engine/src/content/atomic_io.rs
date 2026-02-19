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
    match fs::remove_file(final_path) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            let _ = fs::remove_file(tmp_path);
            return Err(error);
        }
    }

    if let Err(error) = fs::rename(tmp_path, final_path) {
        let _ = fs::remove_file(tmp_path);
        return Err(error);
    }
    Ok(())
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
