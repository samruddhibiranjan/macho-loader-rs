use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut lib_path = PathBuf::from(manifest_dir);

    lib_path.push("cxx_libtlv");

    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-lib=static=tlv");
    println!("cargo:rerun-if-changed=cxx_libtlv/libtlv.a");
}
