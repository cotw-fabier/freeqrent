//! Writes the generated files the bindings read: the schema
//! (`schema/freeqrent.schema.json`) and the conformance suite
//! (`conformance/cases.json`). A changed hash in the suite is an output
//! change; say why in the wave log.
//!
//! ```sh
//! cargo run -p freeqrent-doc --example generate
//! ```

#[path = "../tests/support/mod.rs"]
mod support;

fn main() -> std::io::Result<()> {
    let root = support::root();
    for (path, text) in [
        (support::SCHEMA, freeqrent_doc::schema::schema_json()),
        (support::SUITE, support::generate()),
    ] {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a directory"))?;
        let old = std::fs::read_to_string(&file).unwrap_or_default();
        if old == text {
            println!("{path}: unchanged");
        } else {
            std::fs::write(&file, text)?;
            println!("{path}: written");
        }
    }
    Ok(())
}
