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
    /// The region_id is percent-encoded by the url crate to ensure URI-safe characters.
    /// While ULIDs only contain alphanumeric characters, this provides defense-in-depth.
    pub(crate) fn to_uri_string(&self) -> String {
        // Parse the host URI using the url crate for proper handling
        let Ok(mut url) = url::Url::parse(self.host_uri.as_str()) else {
            // Fallback for unparseable URIs (shouldn't happen with valid file:// URIs)
            return self.host_uri.to_string();
        };

        // Get file extension for the language
        let extension = Self::language_to_extension(&self.language);

        // Build the virtual filename (url crate handles percent-encoding)
        let virtual_filename = format!("{VIRTUAL_URI_PREFIX}{}.{extension}", self.region_id);

        // Use url crate's path_segments_mut to properly handle the path
        // This correctly handles edge cases like root paths and percent-encoding
        if let Ok(mut segments) = url.path_segments_mut() {
            segments.pop(); // Remove the host filename
            segments.push(&virtual_filename); // Add the virtual filename
        }

        url.to_string()
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

    // ==========================================================================
    // cannot-be-a-base URI fallback tests
    // ==========================================================================

    #[test]
    fn to_uri_string_falls_back_to_kakehashi_scheme_for_cannot_be_a_base_uri() {
        // "untitled:Untitled-1" is a cannot-be-a-base URI (no authority, opaque path)
        // These are used by VSCode for unsaved files
        let host_uri: Uri = "untitled:Untitled-1".parse().unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "REGION123");

        let uri_string = virtual_uri.to_uri_string();

        // Should use kakehashi:// scheme as fallback
        assert!(
            uri_string.starts_with("kakehashi:///virtual/"),
            "Cannot-be-a-base URI should fall back to kakehashi:// scheme: {}",
            uri_string
        );
        // Should contain encoded host URI for traceability
        assert!(
            uri_string.contains("untitled"),
            "Fallback URI should contain encoded host URI: {}",
            uri_string
        );
        // Should end with proper extension
        assert!(
            uri_string.ends_with(".lua"),
            "Fallback URI should have language extension: {}",
            uri_string
        );
        // Should contain the virtual filename with region_id
        assert!(
            uri_string.contains("kakehashi-virtual-uri-REGION123"),
            "Fallback URI should contain virtual filename: {}",
            uri_string
        );
    }

    #[test]
    fn to_uri_string_fallback_produces_valid_uri() {
        let host_uri: Uri = "untitled:Untitled-1".parse().unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "python", "REGION456");

        let uri_string = virtual_uri.to_uri_string();

        // Verify the output is a valid URL that can be parsed
        let parsed = Url::parse(&uri_string);
        assert!(
            parsed.is_ok(),
            "Fallback URI should be a valid URL: {}",
            uri_string
        );

        let parsed = parsed.unwrap();
        assert_eq!(parsed.scheme(), "kakehashi");
    }

    #[test]
    fn to_uri_string_fallback_is_detected_as_virtual_uri() {
        let host_uri: Uri = "untitled:Untitled-1".parse().unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "REGION789");

        let uri_string = virtual_uri.to_uri_string();

        // The fallback URI should still be detected by is_virtual_uri
        assert!(
            VirtualDocumentUri::is_virtual_uri(&uri_string),
            "Fallback URI should be detected as virtual: {}",
            uri_string
        );
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

        // Filename without extension after the prefix is rejected
        // Valid format: kakehashi-virtual-uri-{region_id}.{ext} requires a dot after the prefix
        assert!(!VirtualDocumentUri::is_virtual_uri(
            "file:///project/kakehashi-virtual-uri-lua"
        ));
    }
}
