//! Helper trait for [`http::HeaderMap`].

use http::header::{AsHeaderName, HeaderMap};

/// Helper functions for [`HeaderMap`].
pub trait HeaderMapHelper {
    /// Get a header value as a `&str`.
    fn get_str<K: AsHeaderName>(&self, key: K) -> Option<&str>;
}

impl HeaderMapHelper for HeaderMap {
    fn get_str<K: AsHeaderName>(&self, key: K) -> Option<&str> {
        self.get(key).and_then(|value| value.to_str().ok())
    }
}
