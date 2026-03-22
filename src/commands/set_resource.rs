use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::config::xresources::get_user_xresources_path;

pub fn set_user_xresource(key: &str, value: &str) -> Result<PathBuf> {
    let xresources_path = get_user_xresources_path();

    if let Some(parent) = xresources_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {:?}", parent))?;
    }

    let content = match File::open(&xresources_path) {
        Ok(mut file) => {
            let mut buf = String::new();
            file.read_to_string(&mut buf)
                .with_context(|| format!("Failed to read file: {:?}", xresources_path))?;
            buf
        }
        Err(_) => String::new(),
    };

    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let mut found = false;
    let key_lower = key.to_lowercase();

    for line in &mut lines {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with("#include") {
            continue;
        }

        if let Some(colon_pos) = trimmed.find(':') {
            let existing_key = trimmed[..colon_pos].trim().to_lowercase();
            if existing_key == key_lower {
                *line = format!("{}: {}", key, value);
                found = true;
                break;
            }
        }
    }

    if !found {
        if let Some(last) = lines.last() {
            if !last.trim().is_empty() {
                lines.push(String::new());
            }
        }
        lines.push(format!("{}: {}", key, value));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&xresources_path)
        .with_context(|| format!("Failed to open file for writing: {:?}", xresources_path))?;

    for (i, line) in lines.iter().enumerate() {
        if i < lines.len() - 1 {
            writeln!(file, "{}", line)
        } else {
            write!(file, "{}", line)
        }
        .with_context(|| format!("Failed to write to file: {:?}", xresources_path))?;
    }

    Ok(xresources_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_temp_config_dir;

    #[test]
    fn set_user_xresource_creates_and_updates() {
        let dir = create_temp_config_dir();
        let nested_path = dir.path().join(".config/regolith3/Xresources");

        unsafe {
            std::env::set_var("HOME", dir.path());
        }

        // Creates new file with a key
        let result = set_user_xresource("test.key", "test_value");
        assert!(result.is_ok());
        assert!(nested_path.exists());

        let content = std::fs::read_to_string(&nested_path).unwrap();
        assert!(content.contains("test.key: test_value"));

        // Appends another key
        let result2 = set_user_xresource("new.key", "new_value");
        assert!(result2.is_ok());

        let content2 = std::fs::read_to_string(&nested_path).unwrap();
        assert!(content2.contains("test.key: test_value"));
        assert!(content2.contains("new.key: new_value"));

        // Updates existing key
        let result3 = set_user_xresource("test.key", "updated_value");
        assert!(result3.is_ok());

        let content3 = std::fs::read_to_string(&nested_path).unwrap();
        assert!(content3.contains("test.key: updated_value"));
        assert!(content3.contains("new.key: new_value"));
    }
}
