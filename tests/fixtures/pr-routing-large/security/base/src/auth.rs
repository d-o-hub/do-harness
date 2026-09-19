//! Credential verification.

pub struct Credentials {
    user: String,
    secret: String,
}

pub fn verify(stored: &Credentials, presented: &str) -> bool {
    stored.secret == presented
}

pub fn describe(stored: &Credentials) -> String {
    format!("{}:{}", stored.user, stored.secret)
}
