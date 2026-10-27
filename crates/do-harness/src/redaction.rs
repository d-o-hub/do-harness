//! Streaming redaction of configured and secret-like environment values.
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Redactor {
    patterns: Arc<Vec<Vec<u8>>>,
}

impl Redactor {
    #[cfg(test)]
    pub(crate) fn synthetic(values: &[&str]) -> Self {
        Self {
            patterns: Arc::new(values.iter().map(|s| s.as_bytes().to_vec()).collect()),
        }
    }
    pub(crate) fn from_environment(extra: &[String]) -> Self {
        let mut patterns: Vec<Vec<u8>> = std::env::vars_os()
            .filter_map(|(key, value)| {
                let name = key.to_str()?;
                let upper = name.to_ascii_uppercase();
                let sensitive = [
                    "TOKEN",
                    "SECRET",
                    "PASSWORD",
                    "PASSWD",
                    "API_KEY",
                    "PRIVATE_KEY",
                    "CREDENTIAL",
                ]
                .iter()
                .any(|word| upper.contains(word))
                    || extra.iter().any(|key| key == name);
                (sensitive && !value.is_empty()).then(|| value.as_encoded_bytes().to_vec())
            })
            .collect();
        patterns.sort_by_key(|p| std::cmp::Reverse(p.len()));
        patterns.dedup();
        Self {
            patterns: Arc::new(patterns),
        }
    }

    pub(crate) fn owned_text(&self, text: String) -> String {
        String::from_utf8_lossy(&self.stream().feed(&text.into_bytes(), true)).into_owned()
    }

    pub(crate) fn text(&self, text: &str) -> String {
        String::from_utf8_lossy(&self.stream().feed(text.as_bytes(), true)).into_owned()
    }

    pub(crate) fn stream(&self) -> Stream {
        Stream {
            patterns: Arc::clone(&self.patterns),
            pending: Vec::new(),
        }
    }
}

pub(crate) struct Stream {
    patterns: Arc<Vec<Vec<u8>>>,
    pending: Vec<u8>,
}

impl Stream {
    /// Holds an incomplete secret prefix across reads; redacts before clipping.
    pub(crate) fn feed(&mut self, bytes: &[u8], eof: bool) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        let mut output = Vec::new();
        let mut cursor = 0;
        while cursor < self.pending.len() {
            let remaining = &self.pending[cursor..];
            if !eof
                && self
                    .patterns
                    .iter()
                    .any(|p| p.len() > remaining.len() && p.starts_with(remaining))
            {
                break;
            }
            if let Some(pattern) = self.patterns.iter().find(|p| remaining.starts_with(p)) {
                output.extend_from_slice(b"[REDACTED]");
                cursor += pattern.len();
            } else {
                output.push(self.pending[cursor]);
                cursor += 1;
            }
        }
        self.pending.drain(..cursor);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_and_overlapping_values_do_not_expose_secret_fragments() {
        let redactor = Redactor {
            patterns: Arc::new(vec![b"private-long".to_vec(), b"private".to_vec()]),
        };
        let mut stream = redactor.stream();
        let mut output = stream.feed(b"before pri", false);
        output.extend(stream.feed(b"vate", false));
        output.extend(stream.feed(b"-long after", true));
        assert_eq!(output, b"before [REDACTED] after");
    }
}
