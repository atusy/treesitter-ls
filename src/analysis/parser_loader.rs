use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use tree_sitter::Language;

/// A wrapper around dynamic library loading for Tree-sitter language parsers
#[derive(Default)]
pub struct ParserLoader {
    /// Cache of loaded libraries to prevent reloading
    loaded_libraries: HashMap<String, Library>,
}

#[derive(Debug)]
pub enum ParserLoadError {
    LibraryLoadError(libloading::Error),
    SymbolNotFound(String),
    CacheError(String),
}

impl fmt::Display for ParserLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParserLoadError::LibraryLoadError(e) => write!(f, "Failed to load library: {e}"),
            ParserLoadError::SymbolNotFound(func) => write!(f, "Symbol not found: {func}"),
            ParserLoadError::CacheError(msg) => write!(f, "Cache error: {msg}"),
        }
    }
}

impl Error for ParserLoadError {}

impl From<libloading::Error> for ParserLoadError {
    fn from(err: libloading::Error) -> Self {
        ParserLoadError::LibraryLoadError(err)
    }
}

impl ParserLoader {
    /// Create a new ParserLoader instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a Tree-sitter language from a dynamic library
    ///
    /// # Arguments
    /// * `path` - Path to the dynamic library file
    /// * `lang_name` - Name of the language (e.g., "rust", "javascript")
    ///
    /// # Returns
    /// The loaded Language or an error
    pub fn load_language(
        &mut self,
        path: &str,
        lang_name: &str,
    ) -> Result<Language, ParserLoadError> {
        // Derive function name from language name using standard convention
        let func_name = format!("tree_sitter_{lang_name}");
        
        // Load the library if not already loaded
        if !self.loaded_libraries.contains_key(lang_name) {
            let library = unsafe { Library::new(path)? };
            self.loaded_libraries.insert(lang_name.to_string(), library);
        }

        // Get the library from cache
        let library = self
            .loaded_libraries
            .get(lang_name)
            .ok_or_else(|| ParserLoadError::CacheError(
                format!("Failed to get library for {lang_name}")
            ))?;

        // Get the language function from the library
        let language_fn: Symbol<unsafe extern "C" fn() -> Language> =
            unsafe { 
                library.get(func_name.as_bytes())
                    .map_err(|_| ParserLoadError::SymbolNotFound(func_name.clone()))?
            };

        // Call the function to get the Language
        let language = unsafe { language_fn() };

        Ok(language)
    }
    
    /// Load a Tree-sitter language with a custom function name
    ///
    /// Use this when the function name doesn't follow the standard convention
    ///
    /// # Arguments
    /// * `path` - Path to the dynamic library file
    /// * `func_name` - Name of the function to load (e.g., "tree_sitter_rust")
    /// * `lang_name` - Name of the language (used for caching)
    ///
    /// # Returns
    /// The loaded Language or an error
    #[allow(dead_code)]
    pub fn load_language_custom(
        &mut self,
        path: &str,
        func_name: &str,
        lang_name: &str,
    ) -> Result<Language, ParserLoadError> {
        // Load the library if not already loaded
        if !self.loaded_libraries.contains_key(lang_name) {
            let library = unsafe { Library::new(path)? };
            self.loaded_libraries.insert(lang_name.to_string(), library);
        }

        // Get the library from cache
        let library = self
            .loaded_libraries
            .get(lang_name)
            .ok_or_else(|| ParserLoadError::CacheError(
                format!("Failed to get library for {lang_name}")
            ))?;

        // Get the language function from the library
        let language_fn: Symbol<unsafe extern "C" fn() -> Language> =
            unsafe { 
                library.get(func_name.as_bytes())
                    .map_err(|_| ParserLoadError::SymbolNotFound(func_name.to_string()))?
            };

        // Call the function to get the Language
        let language = unsafe { language_fn() };

        Ok(language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_loader_creation() {
        let loader = ParserLoader::new();
        assert!(loader.loaded_libraries.is_empty());
    }
    
    #[test]
    fn test_error_display() {
        let err = ParserLoadError::SymbolNotFound("tree_sitter_rust".to_string());
        assert_eq!(err.to_string(), "Symbol not found: tree_sitter_rust");
        
        let err = ParserLoadError::CacheError("test error".to_string());
        assert_eq!(err.to_string(), "Cache error: test error");
    }
}
