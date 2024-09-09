use std::fmt;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct WorkspaceSummaryOption {
    pub slug: String,
    pub name: Option<Option<String>>,
    pub group_id: String,
}

// Implementing Display trait for WorkspaceSummaryOption to display meaningful text in the Select prompt
impl fmt::Display for WorkspaceSummaryOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(Some(name)) => write!(f, "{} ({})", name, self.slug),
            Some(None) | None => write!(f, "{}", self.slug),
        }
    }
}
