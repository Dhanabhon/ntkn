use ignore::WalkBuilder;
use std::path::Path;

pub struct ProjectScanner;

impl ProjectScanner {
    pub fn scan_project<P: AsRef<Path>>(root: P) -> String {
        let mut combined_text = String::new();
        let walker = WalkBuilder::new(root).build();

        for result in walker {
            if let Ok(entry) = result {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        combined_text.push_str(&content);
                        combined_text.push('\n');
                    }
                }
            }
        }
        combined_text
    }
}