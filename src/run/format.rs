//! Shared command display formatting for resolved plans and hooks.

/// Format a display string from program and args.
pub(super) fn format_display(program: &str, args: &[String]) -> String {
    let mut parts = vec![program.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_program_and_args() {
        let display = format_display(
            "cargo",
            &[
                "test".to_string(),
                "--".to_string(),
                "--nocapture".to_string(),
            ],
        );
        assert_eq!(display, "cargo test -- --nocapture");
    }
}
