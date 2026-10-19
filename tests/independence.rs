//! Ogham depends on nothing of the router, in any dependency table and at
//! any depth.
//!
//! The router (`structure`, working name) lives in its own repository and
//! the two are independent libraries; what joins them lives in a consumer
//! (lorekeeper's `driver`). `cargo tree` is the proof, but a test cannot
//! shell out to cargo cheaply, so this reads the two files `cargo tree`
//! reads: the manifest for a direct edge, and the lockfile, which names
//! every package the build resolves, for a transitive one.

/// Packages that must never appear in this crate's build. `contract` is
/// the document contract, which depends on the router.
const FORBIDDEN: &[&str] = &["structure", "contract"];

fn read(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_manifest_declares_no_router_dependency() {
    let manifest = read("Cargo.toml");
    let mut table = String::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            table = line.to_string();
            continue;
        }
        if !table.contains("dependencies") {
            continue;
        }
        let key = line.split('=').next().unwrap_or("").trim();
        assert!(
            !FORBIDDEN.contains(&key),
            "{table} declares `{key}`: ogham must not depend on the router"
        );
    }
}

#[test]
fn the_lockfile_resolves_no_router_package() {
    let lock = read("Cargo.lock");
    for forbidden in FORBIDDEN {
        let entry = format!("name = \"{forbidden}\"");
        assert!(
            !lock.lines().any(|line| line.trim() == entry),
            "Cargo.lock resolves `{forbidden}`: something pulls the router into ogham's build"
        );
    }
}
