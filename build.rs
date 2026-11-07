use std::{env, fs, path::PathBuf};

fn main() {
    bundle("maps", "txt", "BUNDLED_MAPS", "bundled_maps.rs");
    bundle("effects", "ron", "BUNDLED_EFFECTS", "bundled_effects.rs");
}

/// Writes `file` in the build's output: a table `table` of every
/// `assets/<directory>/*.<extension>`, by name (its file name without the
/// extension), with its text.
fn bundle(directory: &str, extension: &str, table: &str, file: &str) {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets");
    let path = root.join(directory);
    // Watching the directory also catches newly added or removed files.
    println!("cargo:rerun-if-changed={}", path.display());
    let mut files: Vec<_> = fs::read_dir(&path)
        .unwrap_or_else(|_| panic!("read assets/{directory}"))
        .map(|entry| entry.expect("read directory entry").path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == extension))
        .collect();
    files.sort();
    let mut source = format!(
        "/// Every file in `assets/{directory}`, by name (its file name without \
         `.{extension}`).\npub const {table}: &[(&str, &str)] = &[\n"
    );
    for path in files {
        let name = path.file_stem().unwrap().to_str().expect("UTF-8 file name");
        source.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.to_str().expect("UTF-8 path")
        ));
    }
    source.push_str("];\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(file);
    fs::write(output, source).expect("write bundled table");
}
