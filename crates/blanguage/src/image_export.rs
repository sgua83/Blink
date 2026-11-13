//! Explicit deployment export; original build inputs remain in the source artifact.
use std::{fs::File, io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn export(source: &Path, output: &Path) -> Result<()> {
    let image = blanguage::build::execution_image(source)
        .map_err(|error| error as Box<dyn std::error::Error>)?;
    // The export owns a new file. Existing build/cache/deployment files stay with their owner.
    let mut file = File::create_new(output)?;
    file.write_all(&image)?;
    let mut result = String::from("{\"kind\":\"execution_image_export\",");
    crate::json::path(&mut result, "source_path", source);
    result.push(',');
    crate::json::path(&mut result, "output_path", output);
    result.push_str(&format!(",\"byte_length\":{}}}", image.len()));
    println!("{result}");
    Ok(())
}
