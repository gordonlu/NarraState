# Third-party software

NarraState source code is distributed under the MIT License. Its Rust and web builds include third-party open-source packages under their respective licenses.

The direct runtime dependencies are listed in `Cargo.toml`, each crate manifest, and `web/package.json`; exact resolved versions are locked in `Cargo.lock` and `web/package-lock.json`. Major projects include Rust, Tokio, Axum, Serde, SQLx, Reqwest, Vue, Vite, Pinia, and Vue Router.

Before publishing a binary or container, review the complete resolved dependency license set and retain all notices required by those packages. This file is an attribution pointer, not a replacement for dependency license texts.

The bundled `rain-gallery` example and repository-authored visual assets are covered by the repository MIT License unless a file states otherwise.
