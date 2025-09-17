use crate::domain::position::Range;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub uri: Url,
    pub range: Range,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DefinitionResponse {
    Locations(Vec<Location>),
}
