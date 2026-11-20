// rust-embed needs `web/dist` to exist. On a fresh checkout without `make web` we create it
// empty, and the server then answers with a "web app not built" page instead of failing to
// compile. Build info comes from the Makefile through NEXTPLANE_COMMIT / NEXTPLANE_BUILD_DATE.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(&dist);
    }
    println!("cargo:rerun-if-changed=../../web/dist");
    println!("cargo:rerun-if-env-changed=NEXTPLANE_COMMIT");
    println!("cargo:rerun-if-env-changed=NEXTPLANE_BUILD_DATE");
}
