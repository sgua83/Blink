//! Explicit deployment export; original build inputs remain in the source artifact.
use std::path::Path;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn export(source: &Path, output: &Path) -> Result<()> {
    let length = blanguage::build::write_execution_image(source, output)
        .map_err(|error| error as Box<dyn std::error::Error>)?;
    let mut result = String::from("{\"kind\":\"execution_image_export\",");
    crate::json::path(&mut result, "source_path", source);
    result.push(',');
    crate::json::path(&mut result, "output_path", output);
    result.push_str(&format!(",\"byte_length\":{}}}", length));
    println!("{result}");
    Ok(())
}
