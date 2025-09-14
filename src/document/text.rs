/// A pure text document without any parsing or state information
#[derive(Clone, Debug)]
pub struct TextDocument {
    text: String,
    version: Option<i32>,
}

impl TextDocument {
    /// Create a new text document
    pub fn new(text: String) -> Self {
        Self {
            text,
            version: None,
        }
    }

    /// Create a new text document with version
    pub fn with_version(text: String, version: i32) -> Self {
        Self {
            text,
            version: Some(version),
        }
    }

    /// Get the text content
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the text content as owned String
    pub fn into_text(self) -> String {
        self.text
    }

    /// Get the document version
    pub fn version(&self) -> Option<i32> {
        self.version
    }

    /// Update the text content
    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    /// Update the version
    pub fn set_version(&mut self, version: Option<i32>) {
        self.version = version;
    }

    /// Get the length in bytes
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Check if the document is empty
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_document_creation() {
        let doc = TextDocument::new("hello world".to_string());
        assert_eq!(doc.text(), "hello world");
        assert_eq!(doc.version(), None);
        assert_eq!(doc.len(), 11);
        assert!(!doc.is_empty());
    }

    #[test]
    fn test_text_document_with_version() {
        let doc = TextDocument::with_version("test".to_string(), 42);
        assert_eq!(doc.text(), "test");
        assert_eq!(doc.version(), Some(42));
    }

    #[test]
    fn test_text_document_mutation() {
        let mut doc = TextDocument::new("initial".to_string());
        doc.set_text("updated".to_string());
        doc.set_version(Some(1));

        assert_eq!(doc.text(), "updated");
        assert_eq!(doc.version(), Some(1));
    }
}
