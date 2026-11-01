/// Keep the same characters as the original Python utility.
pub fn sanitize_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .filter(|character| {
            character.is_alphanumeric()
                || matches!(character, ' ' | '_' | '-' | '(' | ')' | '&' | '.')
        })
        .collect::<String>()
        .trim()
        .to_owned();

    if sanitized.is_empty() {
        "Untitled".to_owned()
    } else {
        sanitized
    }
}

pub fn numbered_name(number: usize, title: &str) -> String {
    format!("{number}_{}", sanitize_name(title))
}

/// The output directory uses a slightly more restrictive character set.
pub fn output_directory_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .filter(|character| {
            character.is_alphanumeric() || matches!(character, ' ' | '_' | '-')
        })
        .collect::<String>()
        .trim()
        .to_owned();

    if sanitized.is_empty() {
        "Untitled".to_owned()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::{numbered_name, output_directory_name, sanitize_name};

    #[test]
    fn sanitizes_bookmark_names() {
        assert_eq!(sanitize_name(" Chapter 1: A/B? "), "Chapter 1 AB");
        assert_eq!(sanitize_name("***"), "Untitled");
        assert_eq!(numbered_name(2, "Part One"), "2_Part One");
    }

    #[test]
    fn sanitizes_output_directory_name() {
        assert_eq!(output_directory_name("A Book: Vol. 2"), "A Book Vol 2");
    }
}
