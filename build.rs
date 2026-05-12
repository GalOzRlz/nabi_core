use std::fs;
use std::io::Write;

fn main() {
    let sound_dir = "src/community_sounds";          // now inside src
    let out_file = format!("{}/mod.rs", sound_dir);

    let mut modules = String::new();

    // Make sure the directory exists (it already does, but safe)
    fs::create_dir_all(sound_dir).ok();

    if let Ok(entries) = fs::read_dir(sound_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().map_or(false, |ext| ext == "rs")
                && path.file_name().unwrap() != "mod.rs"
            {
                let stem = path.file_stem().unwrap().to_str().unwrap();
                modules.push_str(&format!("pub mod {};\n", stem));
            }
        }
    }

    fs::write(&out_file, modules).unwrap();

    // Re‑run build script when any .rs file in that directory changes
    println!("cargo:rerun-if-changed={}", sound_dir);
}