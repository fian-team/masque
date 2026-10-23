//! Hot-reload file watching.
//!
//! Under emscripten there is no OS to watch: files live in the in-memory
//! filesystem and change only when the host writes them, so the watcher
//! keeps the same interface and never reports a change. `Masque::watch`
//! there is load-from-file; a browser host reloads by building again.

#[cfg(not(target_os = "emscripten"))]
use std::sync::mpsc;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[cfg(not(target_os = "emscripten"))]
use notify::{Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::runtime::error::RuntimeError;

pub struct FileWatcher {
    #[cfg(not(target_os = "emscripten"))]
    _watcher: RecommendedWatcher,
    #[cfg(not(target_os = "emscripten"))]
    receiver: mpsc::Receiver<Result<NotifyEvent, notify::Error>>,
    /// Canonical paths of all files we care about (main file + every imported file).
    #[cfg_attr(target_os = "emscripten", allow(dead_code))]
    watched_paths: HashSet<PathBuf>,
}

impl FileWatcher {
    /// Watch all given paths; any change to any of them will trigger a rerender.
    /// Paths are canonicalized and must exist.
    pub fn new<P, I>(paths: I) -> Result<Self, RuntimeError>
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = P>,
    {
        let mut watched_paths = HashSet::new();
        let mut parents_to_watch: HashSet<PathBuf> = HashSet::new();

        for path in paths {
            let path_buf = PathBuf::from(path.as_ref());

            if !path_buf.exists() {
                return Err(RuntimeError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("File not found: {}", path_buf.display()),
                )));
            }

            let canonical = path_buf.canonicalize().map_err(|e| {
                RuntimeError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to canonicalize {}: {}", path_buf.display(), e),
                ))
            })?;
            watched_paths.insert(canonical.clone());

            if let Some(parent) = canonical.parent() {
                let parent_buf = parent.to_path_buf();
                parents_to_watch.insert(parent_buf);
            }
        }

        #[cfg(target_os = "emscripten")]
        {
            let _ = parents_to_watch;
            Ok(Self { watched_paths })
        }
        #[cfg(not(target_os = "emscripten"))]
        Self::watch_parents(watched_paths, parents_to_watch)
    }

    #[cfg(not(target_os = "emscripten"))]
    fn watch_parents(
        watched_paths: HashSet<PathBuf>,
        parents_to_watch: HashSet<PathBuf>,
    ) -> Result<Self, RuntimeError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(tx).map_err(|e| {
            RuntimeError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create file watcher: {}", e),
            ))
        })?;

        for parent in parents_to_watch {
            watcher
                .watch(&parent, RecursiveMode::NonRecursive)
                .map_err(|e| {
                    RuntimeError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to watch directory: {}", e),
                    ))
                })?;
        }

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
            watched_paths,
        })
    }

    #[cfg(target_os = "emscripten")]
    pub fn check_for_changes(&self) -> bool {
        false
    }

    #[cfg(not(target_os = "emscripten"))]
    pub fn check_for_changes(&self) -> bool {
        let mut file_changed = false;

        while let Ok(Ok(event)) = self.receiver.try_recv() {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    for p in &event.paths {
                        let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
                        if self.watched_paths.contains(&canonical) {
                            file_changed = true;
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        file_changed
    }

    pub fn paths(&self) -> &HashSet<PathBuf> {
        &self.watched_paths
    }
}
