//! Explicit deployment export; original build inputs remain in the source artifact.
use std::{fs::File, io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn export(args: &[std::ffi::OsString]) -> Result<()> {
    if args.len() != 4 || args[2] != "-o" {
        return Err("用法：bc image <entry.blbi> -o <deployment.blri>".into());
    }
    let image = blanguage::build::execution_image(Path::new(&args[1]))
        .map_err(|error| error as Box<dyn std::error::Error>)?;
    // The export owns a new file. Existing build/cache/deployment files stay with their owner.
    let mut output = File::create_new(&args[3])?;
    output.write_all(&image)?;
    let mut result = String::from("{\"kind\":\"execution_image_export\",");
    crate::json::path(&mut result, "source_path", Path::new(&args[1]));
    result.push(',');
    crate::json::path(&mut result, "output_path", Path::new(&args[3]));
    result.push_str(&format!(",\"byte_length\":{}}}", image.len()));
    println!("{result}");
    Ok(())
}
