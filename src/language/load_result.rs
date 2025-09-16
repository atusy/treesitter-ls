/// Results from language loading operations
#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogMessage {
    pub level: LogLevel,
    pub message: String,
}

impl LogMessage {
    pub fn info(message: String) -> Self {
        Self {
            level: LogLevel::Info,
            message,
        }
    }

    pub fn warning(message: String) -> Self {
        Self {
            level: LogLevel::Warning,
            message,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            level: LogLevel::Error,
            message,
        }
    }
}

/// Result of language loading operations
pub struct LanguageLoadResult {
    pub success: bool,
    pub logs: Vec<LogMessage>,
    pub needs_semantic_refresh: bool,
}

impl LanguageLoadResult {
    pub fn new() -> Self {
        Self {
            success: true,
            logs: Vec::new(),
            needs_semantic_refresh: false,
        }
    }

    pub fn success() -> Self {
        Self::new()
    }

    pub fn with_log(mut self, log: LogMessage) -> Self {
        self.logs.push(log);
        self
    }

    pub fn with_logs(mut self, mut logs: Vec<LogMessage>) -> Self {
        self.logs.append(&mut logs);
        self
    }

    pub fn with_semantic_refresh(mut self) -> Self {
        self.needs_semantic_refresh = true;
        self
    }

    pub fn failed(mut self) -> Self {
        self.success = false;
        self
    }

    pub fn merge(mut self, other: LanguageLoadResult) -> Self {
        self.success = self.success && other.success;
        self.logs.extend(other.logs);
        self.needs_semantic_refresh = self.needs_semantic_refresh || other.needs_semantic_refresh;
        self
    }
}

impl Default for LanguageLoadResult {
    fn default() -> Self {
        Self::new()
    }
}