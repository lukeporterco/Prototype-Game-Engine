use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpriteKeyError {
    #[error("sprite key must not be empty")]
    Empty,
    #[error("sprite key must not start with '/'")]
    LeadingSlash,
    #[error("sprite key must not contain '\\\\'")]
    Backslash,
    #[error("sprite key must not contain '..'")]
    ParentTraversal,
    #[error("sprite key contains invalid character '{character}'")]
    InvalidCharacter { character: char },
}

pub(crate) fn validate_sprite_key(key: &str) -> Result<(), SpriteKeyError> {
    if key.is_empty() {
        return Err(SpriteKeyError::Empty);
    }
    if key.starts_with('/') {
        return Err(SpriteKeyError::LeadingSlash);
    }
    if key.contains('\\') {
        return Err(SpriteKeyError::Backslash);
    }
    if key.contains("..") {
        return Err(SpriteKeyError::ParentTraversal);
    }
    for ch in key.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '/' | '-') {
            continue;
        }
        return Err(SpriteKeyError::InvalidCharacter { character: ch });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_sprite_key;

    #[test]
    fn accepts_valid_keys() {
        for key in ["player", "ui/icons/worker_1", "a-b/c_d"] {
            assert!(validate_sprite_key(key).is_ok(), "key={key}");
        }
    }

    #[test]
    fn rejects_invalid_keys() {
        for key in ["", "/a", "..", "a/../b", r"a\b", "A", "a.b"] {
            assert!(validate_sprite_key(key).is_err(), "key={key}");
        }
    }
}
