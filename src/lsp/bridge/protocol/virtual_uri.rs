//! Virtual document URI for injection regions.
//!
//! This module provides the `VirtualDocumentUri` type for encoding host URI,
//! injection language, and region ID into a file:// URI that downstream
//! language servers can use to identify virtual documents.

/// Prefix used for virtual document filenames.
///
/// This distinctive prefix identifies virtual URIs and prevents collisions with real files.
const VIRTUAL_URI_PREFIX: &str = "kakehashi-virtual-uri-";

/// Virtual document URI for injection regions.
///
/// Encodes host URI + injection language + region ID into a file:// URI
/// that downstream language servers can use to identify virtual documents.
///
/// Format: `file:///{host_dir}/kakehashi-virtual-uri-{region_id}.{ext}`
///
/// Example: `file:///project/docs/kakehashi-virtual-uri-01ARZ3NDEKTSV4.lua`
///
/// The file:// scheme is used for compatibility with language servers that
/// only support file:// URIs (e.g., lua-language-server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtualDocumentUri {
    host_uri: tower_lsp_server::ls_types::Uri,
    language: String,
    region_id: String,
}

impl VirtualDocumentUri {
    /// Create a new virtual document URI for an injection region.
    ///
    /// # Arguments
    /// * `host_uri` - The URI of the host document (e.g., markdown file)
    /// * `language` - The injection language (e.g., "lua", "python"). Must not be empty.
    /// * `region_id` - Unique identifier for this injection region within the host. Must not be empty.
    ///
    /// # Panics (debug builds only)
    /// Panics if `language` or `region_id` is empty. These are programming errors
    /// as callers should always provide valid identifiers.
    ///
    /// # Upstream Guarantees
    /// In practice, these parameters are guaranteed valid by upstream sources:
    /// - `region_id` comes from ULID generation (26-char alphanumeric strings)
    /// - `language` comes from Tree-sitter injection queries (non-empty language names)
    ///
    /// In release builds, invalid inputs are accepted without validation to avoid
    /// runtime overhead. Unknown languages produce `.txt` extensions as a safe fallback.
    pub(crate) fn new(
        host_uri: &tower_lsp_server::ls_types::Uri,
        language: &str,
        region_id: &str,
    ) -> Self {
        debug_assert!(!language.is_empty(), "language must not be empty");
        debug_assert!(!region_id.is_empty(), "region_id must not be empty");

        Self {
            host_uri: host_uri.clone(),
            language: language.to_string(),
            region_id: region_id.to_string(),
        }
    }

    /// Get the region_id.
    pub(crate) fn region_id(&self) -> &str {
        &self.region_id
    }

    /// Get the language.
    pub(crate) fn language(&self) -> &str {
        &self.language
    }

    /// Check if a URI string represents a virtual document.
    ///
    /// Virtual document URIs have the filename pattern `kakehashi-virtual-uri-{region_id}.{ext}`.
    /// This is used to distinguish virtual URIs from real file URIs in responses.
    ///
    /// Checks the filename (not path) for the `kakehashi-virtual-uri-` prefix with at least
    /// one `.` for the extension. The prefix is distinctive enough to avoid false positives
    /// with real files.
    pub(crate) fn is_virtual_uri(uri: &str) -> bool {
        // Extract filename from URI
        let filename = uri.rsplit('/').next().unwrap_or("");

        // Check for {VIRTUAL_URI_PREFIX}{region_id}.{ext} pattern
        // Needs at least one dot after the prefix for the extension
        filename.starts_with(VIRTUAL_URI_PREFIX)
            && filename[VIRTUAL_URI_PREFIX.len()..].contains('.')
    }

    /// Extract the parent directory path from a file URI.
    ///
    /// Given a URI like `file:///project/docs/README.md`, returns `/project/docs`.
    /// For root-level files like `file:///README.md`, returns empty string (caller adds `/`).
    ///
    /// Preserves percent-encoding in the path (e.g., `%20` for spaces).
    fn extract_parent_directory(uri_str: &str) -> &str {
        // Strip "file://" prefix to get the path
        let path = uri_str.strip_prefix("file://").unwrap_or(uri_str);

        // Find the last '/' to get the parent directory
        match path.rfind('/') {
            Some(0) => "", // Root directory (e.g., "/README.md" -> "" so file:// + "" + / works)
            Some(pos) => &path[..pos],
            None => "", // Fallback to root if no slash found
        }
    }

    /// Convert to a URI string.
    ///
    /// Format: `file:///{host_dir}/kakehashi-virtual-uri-{region_id}.{ext}`
    ///
    /// Uses file:// scheme placing the virtual file in the same directory as the host
    /// document. This enables downstream language servers to:
    /// - Resolve relative imports (e.g., `from .utils import foo`)
    /// - Find project configuration files (pyproject.toml, tsconfig.json, etc.)
    ///
    /// The `kakehashi-virtual-uri-` prefix is distinctive and unlikely to conflict with
    /// real files. The region_id (ULID) provides global uniqueness.
    ///
    /// The file extension is derived from the language to help downstream language servers
    /// recognize the file type (e.g., lua-language-server needs `.lua` extension).
    ///
    /// The region_id is percent-encoded to ensure URI-safe characters. While ULIDs
    /// only contain alphanumeric characters, this provides defense-in-depth.
    pub(crate) fn to_uri_string(&self) -> String {
        // Get file extension for the language
        let extension = Self::language_to_extension(&self.language);

        // Percent-encode region_id to ensure URI-safe characters
        // RFC 3986 unreserved characters: A-Z a-z 0-9 - . _ ~
        let encoded_region_id = Self::percent_encode_path_segment(&self.region_id);

        // Extract parent directory from host URI
        let parent_path = Self::extract_parent_directory(self.host_uri.as_str());

        // Create a file:// URI with virtual file in host's directory
        // Format: file:///{parent_path}/{VIRTUAL_URI_PREFIX}{region_id}.{ext}
        format!("file://{parent_path}/{VIRTUAL_URI_PREFIX}{encoded_region_id}.{extension}")
    }

    /// Percent-encode a string for use in a URI path segment.
    ///
    /// Encodes all characters except RFC 3986 unreserved characters:
    /// `A-Z a-z 0-9 - . _ ~`
    ///
    /// Multi-byte UTF-8 characters are encoded byte-by-byte, producing valid
    /// percent-encoded sequences (e.g., "日" → "%E6%97%A5").
    ///
    /// # Note
    /// This function is primarily used for defense-in-depth since region_id values
    /// are ULIDs (alphanumeric only), but it ensures URI safety if the format changes.
    pub(super) fn percent_encode_path_segment(s: &str) -> String {
        let mut encoded = String::with_capacity(s.len());
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    encoded.push(byte as char);
                }
                _ => {
                    encoded.push_str(&format!("%{:02X}", byte));
                }
            }
        }
        encoded
    }

    /// Map language name to file extension.
    ///
    /// Downstream language servers (e.g., lua-language-server) often use file extension
    /// to determine file type and enable appropriate language features.
    ///
    /// # Returns
    /// The file extension (without leading dot) for the given language.
    /// Returns "txt" for unknown languages as a safe fallback that won't
    /// trigger any language-specific behavior in downstream servers.
    fn language_to_extension(language: &str) -> &'static str {
        match language {
            "lua" => "lua",
            "python" => "py",
            "rust" => "rs",
            "javascript" => "js",
            "typescript" => "ts",
            "go" => "go",
            "c" => "c",
            "cpp" => "cpp",
            "java" => "java",
            "ruby" => "rb",
            "php" => "php",
            "swift" => "swift",
            "kotlin" => "kt",
            "scala" => "scala",
            "haskell" => "hs",
            "ocaml" => "ml",
            "elixir" => "ex",
            "erlang" => "erl",
            "clojure" => "clj",
            "r" => "r",
            "julia" => "jl",
            "sql" => "sql",
            "html" => "html",
            "css" => "css",
            "json" => "json",
            "yaml" => "yaml",
            "toml" => "toml",
            "xml" => "xml",
            "markdown" => "md",
            "bash" | "sh" => "sh",
            "powershell" => "ps1",
            _ => "txt", // Default fallback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::Uri;
    use url::Url;

    // Helper function to convert url::Url to tower_lsp_server::ls_types::Uri for tests
    fn url_to_uri(url: &Url) -> Uri {
        crate::lsp::lsp_impl::url_to_uri(url).expect("test URL should convert to URI")
    }

    // ==========================================================================
    // extract_parent_directory tests
    // ==========================================================================

    #[test]
    fn extract_parent_directory_from_file_uri() {
        assert_eq!(
            VirtualDocumentUri::extract_parent_directory("file:///project/docs/README.md"),
            "/project/docs"
        );
    }

    #[test]
    fn extract_parent_directory_from_root_file() {
        // Root returns empty string so file:// + "" + / = file:///
        assert_eq!(
            VirtualDocumentUri::extract_parent_directory("file:///README.md"),
            ""
        );
    }

    #[test]
    fn extract_parent_directory_preserves_percent_encoding() {
        assert_eq!(
            VirtualDocumentUri::extract_parent_directory("file:///my%20project/docs/file.md"),
            "/my%20project/docs"
        );
    }

    #[test]
    fn extract_parent_directory_windows_path() {
        assert_eq!(
            VirtualDocumentUri::extract_parent_directory("file:///C:/Users/dev/project/file.md"),
            "/C:/Users/dev/project"
        );
    }

    // ==========================================================================
    // to_uri_string tests
    // ==========================================================================

    #[test]
    fn to_uri_string_uses_host_directory() {
        let host_uri = Url::parse("file:///project/docs/README.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "01ARZ3NDEKTSV4");

        let uri_string = virtual_uri.to_uri_string();

        // Should be in same directory as host, with kakehashi-virtual-uri- prefix
        assert!(
            uri_string.starts_with("file:///project/docs/kakehashi-virtual-uri-"),
            "URI should be in host directory with kakehashi-virtual-uri- prefix: {}",
            uri_string
        );
        assert!(
            uri_string.ends_with(".lua"),
            "URI should have .lua extension: {}",
            uri_string
        );
        // Verify full format
        assert_eq!(
            uri_string, "file:///project/docs/kakehashi-virtual-uri-01ARZ3NDEKTSV4.lua",
            "URI should follow format: file:///<host_dir>/kakehashi-virtual-uri-<region_id>.<ext>"
        );
    }

    #[test]
    fn to_uri_string_root_level_host() {
        let host_uri = Url::parse("file:///README.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "python", "REGION123");

        let uri_string = virtual_uri.to_uri_string();

        assert_eq!(
            uri_string, "file:///kakehashi-virtual-uri-REGION123.py",
            "Root-level host should produce root-level virtual URI"
        );
    }

    #[test]
    fn to_uri_string_preserves_percent_encoding_in_path() {
        let host_uri = Url::parse("file:///my%20project/docs/file.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "REGION1");

        let uri_string = virtual_uri.to_uri_string();

        assert!(
            uri_string.starts_with("file:///my%20project/docs/kakehashi-virtual-uri-"),
            "URI should preserve percent-encoding in path: {}",
            uri_string
        );
    }

    #[test]
    fn includes_language_extension() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.ends_with(".lua"),
            "URI should have .lua extension: {}",
            uri_string
        );
    }

    #[test]
    fn region_id_accessor_returns_stored_value() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri =
            VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "01ARZ3NDEKTSV4RRFFQ69G5FAV");

        assert_eq!(virtual_uri.region_id(), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    }

    #[test]
    fn language_accessor_returns_stored_value() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "python", "region-0");

        assert_eq!(virtual_uri.language(), "python");
    }

    #[test]
    fn percent_encodes_special_characters_in_region_id() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Test with characters that need encoding: space, slash, question mark
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region/0?test");

        let uri_string = virtual_uri.to_uri_string();
        // "/" should be encoded as %2F, "?" should be encoded as %3F
        assert!(
            uri_string.contains("region%2F0%3Ftest"),
            "Special characters should be percent-encoded: {}",
            uri_string
        );
    }

    #[test]
    fn preserves_alphanumeric_and_safe_chars_in_region_id() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // RFC 3986 unreserved characters: A-Z a-z 0-9 - . _ ~
        let virtual_uri =
            VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "ABC-xyz_123.test~v2");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.contains("ABC-xyz_123.test~v2.lua"),
            "Unreserved characters should not be encoded: {}",
            uri_string
        );
    }

    #[test]
    fn same_inputs_produce_same_output() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-0");

        assert_eq!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Same inputs should produce deterministic output"
        );
    }

    #[test]
    fn different_region_ids_produce_different_uris() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-1");

        assert_ne!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Different region_ids should produce different URIs"
        );
    }

    #[test]
    fn different_languages_produce_different_extensions() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let lua_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-0");
        let python_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "python", "region-0");

        assert!(lua_uri.to_uri_string().ends_with(".lua"));
        assert!(python_uri.to_uri_string().ends_with(".py"));
    }

    #[test]
    fn language_to_extension_maps_common_languages() {
        // Test a representative sample of the supported languages
        let test_cases = [
            ("lua", "lua"),
            ("python", "py"),
            ("rust", "rs"),
            ("javascript", "js"),
            ("typescript", "ts"),
            ("go", "go"),
            ("c", "c"),
            ("cpp", "cpp"),
            ("java", "java"),
            ("ruby", "rb"),
            ("bash", "sh"),
            ("sh", "sh"),
        ];

        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        for (language, expected_ext) in test_cases {
            let uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), language, "region-0");
            let uri_string = uri.to_uri_string();
            assert!(
                uri_string.ends_with(&format!(".{}", expected_ext)),
                "Language '{}' should produce extension '{}', got: {}",
                language,
                expected_ext,
                uri_string
            );
        }
    }

    #[test]
    fn language_to_extension_falls_back_to_txt_for_unknown() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Unknown languages should default to .txt
        let unknown_cases = ["unknown-lang", "foobar", "notareallan"];

        for language in unknown_cases {
            let uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), language, "region-0");
            let uri_string = uri.to_uri_string();
            assert!(
                uri_string.ends_with(".txt"),
                "Unknown language '{}' should produce .txt extension, got: {}",
                language,
                uri_string
            );
        }
    }

    #[test]
    fn different_hosts_in_same_directory_produce_same_virtual_directory() {
        // With the new format, virtual URIs are in the same directory as the host
        // Two hosts in the same directory will have virtual files in the same directory
        let host1 = Url::parse("file:///project/doc1.md").unwrap();
        let host2 = Url::parse("file:///project/doc2.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&url_to_uri(&host1), "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&url_to_uri(&host2), "lua", "region-0");

        // They should still produce identical URIs since same region_id and language
        assert_eq!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Same region_id in same directory should produce identical URIs"
        );
    }

    #[test]
    fn different_directories_produce_different_virtual_uris() {
        let host1 = Url::parse("file:///project/dir1/doc.md").unwrap();
        let host2 = Url::parse("file:///project/dir2/doc.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&url_to_uri(&host1), "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&url_to_uri(&host2), "lua", "region-0");

        assert_ne!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Different host directories should produce different virtual URIs"
        );
    }

    #[test]
    fn equality_checks_all_fields() {
        let host1 = Url::parse("file:///project/doc1.md").unwrap();
        let host2 = Url::parse("file:///project/doc2.md").unwrap();

        let uri1 = VirtualDocumentUri::new(&url_to_uri(&host1), "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&url_to_uri(&host1), "lua", "region-0");
        let uri3 = VirtualDocumentUri::new(&url_to_uri(&host2), "lua", "region-0");
        let uri4 = VirtualDocumentUri::new(&url_to_uri(&host1), "python", "region-0");
        let uri5 = VirtualDocumentUri::new(&url_to_uri(&host1), "lua", "region-1");

        assert_eq!(uri1, uri2, "Same fields should be equal");
        assert_ne!(uri1, uri3, "Different host_uri should not be equal");
        assert_ne!(uri1, uri4, "Different language should not be equal");
        assert_ne!(uri1, uri5, "Different region_id should not be equal");
    }

    #[test]
    #[should_panic(expected = "language must not be empty")]
    #[cfg(debug_assertions)]
    fn panics_on_empty_language_in_debug() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let _ = VirtualDocumentUri::new(&url_to_uri(&host_uri), "", "region-0");
    }

    #[test]
    #[should_panic(expected = "region_id must not be empty")]
    #[cfg(debug_assertions)]
    fn panics_on_empty_region_id_in_debug() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let _ = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "");
    }

    #[test]
    fn percent_encode_preserves_unreserved_characters() {
        // RFC 3986 unreserved: ALPHA / DIGIT / "-" / "." / "_" / "~"
        let input = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, input,
            "Unreserved characters should not be encoded"
        );
    }

    #[test]
    fn percent_encode_encodes_reserved_characters() {
        // Some reserved characters that need encoding in path segments
        let input = "test/path?query#fragment";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "test%2Fpath%3Fquery%23fragment",
            "Reserved characters should be percent-encoded"
        );
    }

    #[test]
    fn percent_encode_encodes_space() {
        let input = "hello world";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(encoded, "hello%20world", "Space should be encoded as %20");
    }

    #[test]
    fn percent_encode_handles_multibyte_utf8() {
        // UTF-8 multi-byte characters should have each byte percent-encoded
        // "日" (U+65E5) = E6 97 A5 in UTF-8
        let input = "日";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "%E6%97%A5",
            "Multi-byte UTF-8 should encode each byte"
        );
    }

    #[test]
    fn percent_encode_handles_mixed_ascii_and_utf8() {
        // Mix of ASCII alphanumerics (preserved) and UTF-8 (encoded)
        let input = "region-日本語-test";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        // "日" = E6 97 A5, "本" = E6 9C AC, "語" = E8 AA 9E
        assert_eq!(
            encoded, "region-%E6%97%A5%E6%9C%AC%E8%AA%9E-test",
            "Mixed content should preserve ASCII and encode UTF-8"
        );
    }

    #[test]
    fn percent_encode_handles_emoji() {
        // Emoji are 4-byte UTF-8 sequences
        // "🦀" (U+1F980) = F0 9F A6 80 in UTF-8
        let input = "rust🦀";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "rust%F0%9F%A6%80",
            "4-byte UTF-8 (emoji) should encode all bytes"
        );
    }

    #[test]
    fn to_uri_string_contains_region_id_in_filename() {
        // Verify that the region_id appears in the URI
        // Format: kakehashi-virtual-uri-{region_id}.{ext}
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let region_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", region_id);

        let uri_string = virtual_uri.to_uri_string();

        // Extract filename from the URI path
        let filename = uri_string.rsplit('/').next().unwrap();
        // Filename format: kakehashi-virtual-uri-{region_id}.{ext}
        // Remove kakehashi-virtual-uri- prefix and .ext suffix
        let without_prefix = filename.strip_prefix("kakehashi-virtual-uri-").unwrap();
        let extracted_id = without_prefix
            .rsplit_once('.')
            .map(|(name, _)| name)
            .unwrap();

        assert_eq!(
            extracted_id, region_id,
            "Region ID should be extractable from URI"
        );
    }

    #[test]
    fn to_uri_string_produces_valid_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();

        // Verify the output is a valid URL
        let parsed = Url::parse(&uri_string);
        assert!(
            parsed.is_ok(),
            "to_uri_string() should produce a valid URL: {}",
            uri_string
        );

        let parsed = parsed.unwrap();
        assert_eq!(parsed.scheme(), "file");
        // Path should be in host directory with kakehashi-virtual-uri- prefix filename
        let filename = parsed.path().rsplit('/').next().unwrap();
        assert!(
            filename.starts_with("kakehashi-virtual-uri-"),
            "Filename should start with kakehashi-virtual-uri-: {}",
            filename
        );
    }

    // ==========================================================================
    // is_virtual_uri tests
    // ==========================================================================

    #[test]
    fn is_virtual_uri_detects_new_format() {
        // New format: kakehashi-virtual-uri-{region_id}.{ext}
        assert!(VirtualDocumentUri::is_virtual_uri(
            "file:///project/kakehashi-virtual-uri-01ARZ3NDEK.lua"
        ));
        assert!(VirtualDocumentUri::is_virtual_uri(
            "file:///project/docs/kakehashi-virtual-uri-REGION123.py"
        ));
        assert!(VirtualDocumentUri::is_virtual_uri(
            "file:///kakehashi-virtual-uri-01JPMQ8ZYYQA.txt"
        ));
        // With special characters in region_id (percent-encoded)
        assert!(VirtualDocumentUri::is_virtual_uri(
            "file:///project/kakehashi-virtual-uri-region%2F0.lua"
        ));
    }

    #[test]
    fn is_virtual_uri_rejects_real_uris() {
        // Normal files
        assert!(!VirtualDocumentUri::is_virtual_uri(
            "file:///home/user/project/main.lua"
        ));
        assert!(!VirtualDocumentUri::is_virtual_uri(
            "file:///C:/Users/dev/code.py"
        ));
        assert!(!VirtualDocumentUri::is_virtual_uri("untitled:Untitled-1"));

        // Real file with .kakehashi in DIRECTORY name (not filename)
        assert!(!VirtualDocumentUri::is_virtual_uri(
            "file:///home/.kakehashi/config.lua"
        ));

        // Real file that starts with "kakehashi" but not "kakehashi-virtual-uri-"
        assert!(!VirtualDocumentUri::is_virtual_uri(
            "file:///project/kakehashi.config.lua"
        ));

        // File with only one dot after .kakehashi (not enough for valid format)
        // Valid format needs: kakehashi-virtual-uri-{region_id}.{ext} = at least 2 dots after .kakehashi
        assert!(!VirtualDocumentUri::is_virtual_uri(
            "file:///project/kakehashi-virtual-uri-lua"
        ));
    }
}
