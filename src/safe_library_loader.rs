use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use tree_sitter::Language;

/// A safe wrapper around dynamic library loading for Tree-sitter language parsers
pub struct LibraryLoader {
    /// Cache of loaded libraries to prevent reloading
    loaded_libraries: HashMap<String, Library>,
}

#[derive(Debug)]
pub struct LoadError {
    message: String,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Library load error: {}", self.message)
    }
}

impl Error for LoadError {}

impl LibraryLoader {
    /// Create a new LibraryLoader instance
    pub fn new() -> Self {
        Self {
            loaded_libraries: HashMap::new(),
        }
    }

    /// Load a Tree-sitter language from a dynamic library
    ///
    /// # Arguments
    /// * `path` - Path to the dynamic library file
    /// * `func_name` - Name of the function to load (e.g., "tree_sitter_rust")
    /// * `lang_name` - Name of the language (used for caching)
    ///
    /// # Returns
    /// The loaded Language or an error
    pub fn load_language(
        &mut self,
        path: &str,
        func_name: &str,
        lang_name: &str,
    ) -> Result<Language, Box<dyn Error>> {
        // Load the library if not already loaded
        if !self.loaded_libraries.contains_key(lang_name) {
            let library = unsafe { Library::new(path)? };
            self.loaded_libraries.insert(lang_name.to_string(), library);
        }

        // Get the library from cache
        let library = self
            .loaded_libraries
            .get(lang_name)
            .ok_or_else(|| LoadError {
                message: format!("Failed to get library for {lang_name}"),
            })?;

        // Get the language function from the library
        let language_fn: Symbol<unsafe extern "C" fn() -> Language> =
            unsafe { library.get(func_name.as_bytes())? };

        // Call the function to get the Language
        let language = unsafe { language_fn() };

        Ok(language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_loader_creation() {
        let loader = LibraryLoader::new();
        assert!(loader.loaded_libraries.is_empty());
    }
}
