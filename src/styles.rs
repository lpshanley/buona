use console::Style;

pub struct Styles {
    pub bold: Style,
    pub dim: Style,
    pub cyan: Style,
    pub green: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            bold: Style::new().bold(),
            dim: Style::new().dim(),
            cyan: Style::new().cyan().bold(),
            green: Style::new().green().bold(),
        }
    }
}
