//! The `VcsPort` trait itself lives outside `git.rs` deliberately: `git.rs`
//! (and its `git2` dependency) is Cargo-feature-gated behind
//! `git2-backend` (see `shiki-core/Cargo.toml`), but `NotebookStore` needs
//! this trait's *type* to always exist regardless of that feature — its
//! `vcs: Arc<dyn VcsPort>` field, and the `new_with_vcs_backend` constructor
//! that accepts any implementation, are how a future non-native consumer
//! (no local git2) supplies its own notebook-detection/init logic without
//! `notebook.rs` itself needing to know or care whether `git2-backend` is
//! on. `git::NativeVcs` (the default implementation, gated the same as the
//! rest of `git.rs`) is what `NotebookStore::new`/`new_with_custom_paths`
//! use when that feature *is* on — which is every existing consumer today.

use std::path::Path;

use crate::Result;

pub trait VcsPort: Send + Sync {
    fn init_repo(&self, path: &Path) -> Result<()>;
    fn is_repo(&self, path: &Path) -> bool;
}
