//! Supabase URL / anon key from environment (testable with `temp-env`).

pub(crate) fn supabase_url() -> String {
    std::env::var("NEXT_PUBLIC_SUPABASE_URL")
        .or_else(|_| std::env::var("VITE_SUPABASE_URL"))
        .unwrap_or_else(|_| "https://dzpyrdxelcgfpmcdojvb.supabase.co".to_string())
}

pub(crate) fn supabase_anon_key() -> String {
    std::env::var("NEXT_PUBLIC_SUPABASE_ANON_KEY")
        .or_else(|_| std::env::var("VITE_SUPABASE_PUBLIC_KEY"))
        .unwrap_or_else(|_| {
            "sb_publishable_Ky02yQS5HHpkmrN1DE2yaw_EwENlsPZ".to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_env_overrides() {
        temp_env::with_vars(
            [
                ("NEXT_PUBLIC_SUPABASE_URL", Some("https://custom.supabase.co")),
                ("NEXT_PUBLIC_SUPABASE_ANON_KEY", Some("pk-test")),
            ],
            || {
                assert_eq!(supabase_url(), "https://custom.supabase.co");
                assert_eq!(supabase_anon_key(), "pk-test");
            },
        );
    }

    #[test]
    fn defaults_when_supabase_env_unset() {
        temp_env::with_vars(
            [
                ("NEXT_PUBLIC_SUPABASE_URL", None::<&str>),
                ("NEXT_PUBLIC_SUPABASE_ANON_KEY", None::<&str>),
            ],
            || {
                assert!(supabase_url().contains("dzpyrdxelcgfpmcdojvb"));
                assert!(supabase_anon_key().starts_with("sb_publishable_"));
            },
        );
    }
}
