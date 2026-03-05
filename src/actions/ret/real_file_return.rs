//! A return of a real, on-disk file.

use super::{
    ActionReturn, ContentReturn, Context, MediaType, RequestContext, Result,
    Return, Source, StringReturn, VariableMap,
};
use axum::body::Body;
use axum::response::Response;
use http::{StatusCode, header};
use jiff::Timestamp;
use std::fs;
use std::io::{self, Read, Seek};
use std::path::Path;

/// A file on the file system.
#[derive(Debug)]
pub struct RealFileReturn {
    /// Path that identified the file.
    ///
    /// This is a [`String`] instead of a [`PathBuf`][std::path::PathBuf]
    /// because we only handle UTF-8 URLs.
    pub inner_path: String,

    /// The open file object.
    pub file: fs::File,

    /// Time the file was last modified.
    pub modified: Option<Timestamp>,

    /// Time the file was created.
    pub created: Option<Timestamp>,
}

impl RealFileReturn {
    /// Create a `RealFileReturn` from a URL path and a file system path.
    ///
    /// `url_path` is the path that would be requested from the web server, e.g.
    /// `"/index.html"`, to get the file, and `fs_path` is the path on the file
    /// system, e.g. `"/srv/website/index.html"`.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if `fs_path` doesn’t exist, isn’t a file, or
    /// otherwise couldn’t be read.
    pub fn new<P: AsRef<Path>>(
        url_path: String,
        fs_path: P,
    ) -> io::Result<Self> {
        let file = open_confirmed_file(fs_path)?;
        let metadata = file.metadata().ok();

        Ok(Self {
            inner_path: url_path,
            file,
            modified: metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| Timestamp::try_from(t).ok()),
            created: metadata
                .as_ref()
                .and_then(|m| m.created().ok())
                .and_then(|t| Timestamp::try_from(t).ok()),
        })
    }

    /// Create a `RealFileReturn` from an inner path.
    ///
    /// A return’s inner path ([`Return::inner_path()`]) might be relative to
    /// the working directory and likely the web server root, or it might be
    /// absolute. It depends on how the path is defined in the configuration.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the equivalent file system path doesn’t exist,
    /// isn’t a file, or otherwise couldn’t be read.
    pub fn from_inner_path<V: VariableMap>(
        path: String,
        context: &Context<V>,
    ) -> io::Result<Self> {
        let fs_path = context.real_path(&path);
        Self::new(path, fs_path)
    }

    /// Create a `RealFileReturn` from a file system path.
    ///
    /// This will use [`Path::display()`] to save the path to file, so this
    /// should only be called from CLI or test code that doesn’t need to make
    /// canonical comparisons against non-UTF-8 paths.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the path doesn’t exist, isn’t a file, or
    /// otherwise couldn’t be read.
    pub fn from_file_system<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::new(path.as_ref().display().to_string(), path)
    }

    /// Build an HTTP response for this file, handling conditional requests.
    ///
    /// Sets `ETag`, `Last-Modified`, and `Content-Type` headers. Returns 304
    /// if the client’s cached version is still fresh.
    ///
    /// # Errors
    ///
    ///   * [`io::Error`]s from reading the file.
    fn into_static_response<V: VariableMap>(
        self,
        context: &Context<V>,
    ) -> Result<Response> {
        let content_type = Self::content_type_for_path(&self.inner_path);
        let etag = self.etag();
        let last_modified = self.last_modified_header();

        let headers = context.request_headers();
        if Self::is_not_modified(
            headers,
            etag.as_deref(),
            last_modified.as_deref(),
        ) {
            return Ok(http::Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .expect("valid response"));
        }

        let Self { mut file, .. } = self;
        let mut body = Vec::new();
        #[expect(
            clippy::verbose_file_reads,
            reason = "file is already open from struct field"
        )]
        file.read_to_end(&mut body)?;

        let mut builder = http::Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, &content_type);
        if let Some(etag) = etag {
            builder = builder.header(header::ETAG, etag);
        }
        if let Some(lm) = last_modified {
            builder = builder.header(header::LAST_MODIFIED, lm);
        }
        Ok(builder.body(Body::from(body)).expect("valid response"))
    }

    /// Detect content-type for a file path, adding `charset=utf-8` for text.
    fn content_type_for_path(path: &str) -> String {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        if mime.type_() == mime::TEXT
            && !mime.params().any(|(name, _)| name == mime::CHARSET)
        {
            format!("{mime}; charset=utf-8")
        } else {
            mime.to_string()
        }
    }

    /// Compute an `ETag` from file metadata.
    fn etag(&self) -> Option<String> {
        self.modified.map(|ts| {
            let size = self.file.metadata().map(|m| m.len()).unwrap_or(0);
            format!("\"{}-{}\"", ts.as_second(), size)
        })
    }

    /// Format `Last-Modified` as an HTTP date string.
    fn last_modified_header(&self) -> Option<String> {
        self.modified.map(|ts| {
            ts.to_zoned(jiff::tz::TimeZone::UTC)
                .strftime("%a, %d %b %Y %H:%M:%S GMT")
                .to_string()
        })
    }

    /// Check `If-None-Match` and `If-Modified-Since` headers for a 304.
    fn is_not_modified(
        headers: Option<&http::HeaderMap>,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> bool {
        let Some(headers) = headers else { return false };

        if let (Some(etag), Some(inm)) = (
            etag,
            headers
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok()),
        ) {
            return inm == "*" || inm.split(',').any(|e| e.trim() == etag);
        }

        if let (Some(lm), Some(ims)) = (
            last_modified,
            headers
                .get(header::IF_MODIFIED_SINCE)
                .and_then(|v| v.to_str().ok()),
        ) {
            return lm == ims;
        }

        false
    }
}

impl Return for RealFileReturn {
    fn inner_path(&self) -> Result<&str> {
        Ok(&self.inner_path)
    }

    fn ensure_file<V: VariableMap>(
        self,
        _context: &Context<V>,
    ) -> Result<ActionReturn> {
        Ok(self.into())
    }

    fn into_string_return(self) -> Result<StringReturn> {
        Ok(StringReturn::from(self.inner_path))
    }

    fn into_content_return<V: VariableMap>(
        self,
        context: &Context<V>,
    ) -> Result<ContentReturn> {
        let Self { mut file, inner_path: url_path, created, modified } = self;

        let mut body =
            String::with_capacity(file.metadata()?.len().try_into().map_err(
                |_| crate::Error::FileTooLarge(context.real_path(&url_path)),
            )?);

        #[expect(
            clippy::verbose_file_reads,
            reason = "required by code that checks if path is a file"
        )]
        file.read_to_string(&mut body)?;

        Ok(ContentReturn {
            body: body.into(),
            content_type: MediaType::APPLICATION_OCTET_STREAM,
            source: Source::File { inner_path: url_path, modified, created },
            ..Default::default()
        })
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<Response> {
        self.into_static_response(context)
    }
}

/// Open a file and confirm that it is a file.
///
/// This reads one byte to check if the file is a directory (using `is_dir()`
/// would create a race condition.)
///
/// Returns the opened file (rewound).
///
/// # Errors
///
///   * [`io::Error`] resulting from opening the file, reading a byte, or
///     seeking to the start of the file.
pub fn open_confirmed_file<P: AsRef<Path>>(path: P) -> io::Result<fs::File> {
    let mut file = fs::File::open(path)?;
    let mut buffer: [u8; 1] = [0];
    _ = file.read(&mut buffer)?;
    file.rewind()?;
    Ok(file)
}

impl From<RealFileReturn> for Result {
    fn from(ret: RealFileReturn) -> Self {
        Ok(ret.into())
    }
}
