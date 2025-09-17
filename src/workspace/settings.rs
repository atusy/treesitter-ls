use crate::config::{TreeSitterSettings, merge_settings};
use crate::domain::settings::WorkspaceSettings;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsEventKind {
    Info,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsEvent {
    pub kind: SettingsEventKind,
    pub message: String,
}

impl SettingsEvent {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            kind: SettingsEventKind::Info,
            message: message.into(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            kind: SettingsEventKind::Warning,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSource {
    InitializationOptions,
    ClientConfiguration,
}

impl SettingsSource {
    fn description(self) -> &'static str {
        match self {
            SettingsSource::InitializationOptions => "initialization options",
            SettingsSource::ClientConfiguration => "client configuration",
        }
    }
}

#[derive(Default, Debug)]
pub struct SettingsLoadOutcome {
    pub settings: Option<WorkspaceSettings>,
    pub events: Vec<SettingsEvent>,
}

pub fn load_settings(
    root_path: Option<&Path>,
    override_settings: Option<(SettingsSource, Value)>,
) -> SettingsLoadOutcome {
    let mut events = Vec::new();

    let file_settings = load_toml_settings(root_path, &mut events);
    let override_settings = override_settings
        .and_then(|(source, value)| parse_override_settings(source, value, &mut events));

    let merged = merge_settings(file_settings, override_settings);
    let settings = merged.map(WorkspaceSettings::from);

    SettingsLoadOutcome { settings, events }
}

fn load_toml_settings(
    root_path: Option<&Path>,
    events: &mut Vec<SettingsEvent>,
) -> Option<TreeSitterSettings> {
    let root = root_path?;
    let config_path = root.join("treesitter-ls.toml");
    if !config_path.exists() {
        return None;
    }

    events.push(SettingsEvent::info(format!(
        "Found config file: {}",
        config_path.display()
    )));

    match fs::read_to_string(&config_path) {
        Ok(contents) => match toml::from_str::<TreeSitterSettings>(&contents) {
            Ok(settings) => {
                events.push(SettingsEvent::info(
                    "Successfully loaded treesitter-ls.toml",
                ));
                Some(settings)
            }
            Err(err) => {
                events.push(SettingsEvent::warning(format!(
                    "Failed to parse treesitter-ls.toml: {}",
                    err
                )));
                None
            }
        },
        Err(err) => {
            events.push(SettingsEvent::warning(format!(
                "Failed to read treesitter-ls.toml: {}",
                err
            )));
            None
        }
    }
}

fn parse_override_settings(
    source: SettingsSource,
    value: Value,
    events: &mut Vec<SettingsEvent>,
) -> Option<TreeSitterSettings> {
    match serde_json::from_value::<TreeSitterSettings>(value) {
        Ok(settings) => {
            events.push(SettingsEvent::info(format!(
                "Parsed {} as TreeSitterSettings",
                source.description()
            )));
            Some(settings)
        }
        Err(err) => {
            events.push(SettingsEvent::warning(format!(
                "Failed to parse {}: {}",
                source.description(),
                err
            )));
            None
        }
    }
}
