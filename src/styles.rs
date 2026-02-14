//! Terminal styling utilities for consistent output formatting.

use console::Style;

#[derive(Debug)]
pub(crate) struct Styles {
    pub(crate) bold: Style,
    pub(crate) dim: Style,
    pub(crate) cyan: Style,
    pub(crate) green: Style,
    pub(crate) red: Style,
    pub(crate) yellow: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            bold: Style::new().bold(),
            dim: Style::new().dim(),
            cyan: Style::new().cyan().bold(),
            green: Style::new().green().bold(),
            red: Style::new().red().bold(),
            yellow: Style::new().yellow().bold(),
        }
    }
}
