use std::fs;
use std::io::{
    ErrorKind,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};

pub(crate) fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let target = resolve_link(path);
    if fs::metadata(&target).is_ok_and(|existing| existing.permissions().readonly()) {
        return Err(ErrorKind::PermissionDenied.into());
    }
    let temp = temp_path(&target);
    let replaced = write_new(&temp, &target, content).and_then(|()| fs::rename(&temp, &target));
    let Err(error) = replaced else {
        return Ok(());
    };
    let _ = fs::remove_file(&temp);
    match error.kind() {
        ErrorKind::ResourceBusy
        | ErrorKind::CrossesDevices
        | ErrorKind::PermissionDenied
        | ErrorKind::ReadOnlyFilesystem => fs::write(&target, content),
        _ => Err(error),
    }
}

fn resolve_link(path: &Path) -> PathBuf {
    if let Ok(target) = fs::canonicalize(path) {
        return target;
    }
    match fs::read_link(path) {
        Ok(link) => path
            .parent()
            .map_or_else(|| link.clone(), |dir| dir.join(&link)),
        Err(_) => path.to_path_buf(),
    }
}

fn write_new(temp: &Path, target: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)?;
    if let Ok(existing) = fs::metadata(target) {
        file.set_permissions(existing.permissions())?;
    }
    file.write_all(content)?;
    file.sync_all()
}

fn temp_path(target: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(format!(".{name}.{}.{unique}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests;
