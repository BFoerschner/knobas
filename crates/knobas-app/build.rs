use std::path::PathBuf;

fn main() {
    ensure_frontend_dist();
    tauri_build::build();
}

/// Make sure `tauri.conf.json`'s `frontendDist` exists before the macro reads
/// it.
///
/// `tauri::generate_context!` embeds the built frontend at compile time, so on
/// a clone where `npm run build` has not run yet, a bare `cargo build`,
/// `cargo clippy` or `cargo test` fails to *compile* this crate -- a confusing
/// way to be told the frontend is missing. `just check` builds the frontend
/// first and this directory is then the real one; an empty directory only ever
/// backs a binary nobody asked to run, and the moment anyone does build the
/// frontend it is populated in place.
fn ensure_frontend_dist() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets this"));
    let dist = manifest.join("../../app/dist");
    if let Err(error) = std::fs::create_dir_all(&dist) {
        println!("cargo:warning=could not create {}: {error}", dist.display());
    }
}
