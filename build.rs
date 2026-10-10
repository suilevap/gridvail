use std::{env, fs, path::PathBuf};

fn main() {
    let directory = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets/maps");
    // Watching the directory also catches newly added or removed maps.
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut maps: Vec<_> = fs::read_dir(&directory)
        .expect("read assets/maps")
        .map(|entry| entry.expect("read map entry").path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    maps.sort();
    let mut source = String::from("const BUNDLED_MAPS: &[(&str, &str)] = &[\n");
    for path in maps {
        let name = path.file_stem().unwrap().to_str().expect("UTF-8 map name");
        source.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.to_str().expect("UTF-8 map path")
        ));
    }
    source.push_str("];\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("bundled_maps.rs");
    fs::write(output, source).expect("write bundled map table");
}
