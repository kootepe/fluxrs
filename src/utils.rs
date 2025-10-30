use std::error::Error;
use std::fs;
use std::path::Path;
use std::str;

pub fn ensure_utf8<P: AsRef<Path>>(path: P) -> Result<String, Box<dyn Error>> {
    let bytes = fs::read(&path)?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => {
            Err(format!("Input file '{}' is not valid UTF-8: {}", path.as_ref().display(), e)
                .into())
        },
    }
}
