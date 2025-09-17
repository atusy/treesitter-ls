use crate::domain::position::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionRange {
    pub range: Range,
    pub parent: Option<Box<SelectionRange>>,
}
