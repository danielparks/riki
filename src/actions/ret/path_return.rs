//! A return of a real, on-disk file.

use super::{
    ActionReturn, ContentReturn, Context, Error, MediaType, Metadata,
    RequestContext, Result, Return, Source, StringReturn, VariableMap,
};
use actix_files::NamedFile;
use actix_web::HttpResponse;
use jiff::Timestamp;
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

/// A file on the file system.
#[derive(Debug)]
pub struct PathReturn {
    /// Path to the file.
    pub path: PathBuf,

    /// The open file object.
    pub file: fs::File,

    /// Time the file was last modified.
    pub modified: Option<Timestamp>,

    /// Time the file was created.
    pub created: Option<Timestamp>,
}

impl PathReturn {
    /// Create a `PathReturn` from a request path.
    ///
    /// Request paths always start with `'/'`, but in this case they are
    /// evaluated as relative paths.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the path doesn’t exist, isn’t a file, or
    /// otherwise couldn’t be read.
    pub fn new<'a, V: VariableMap<'a>>(
        path: PathBuf,
        context: &'a Context<'a, V>,
    ) -> io::Result<Self> {
        let file = open_confirmed_file(context.real_path(&path))?;
        let metadata = file.metadata().ok();

        Ok(Self {
            path,
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

    /// Convert to a [`NamedFile`].
    ///
    /// This makes all `text/*` files default to having `charset=utf-8`. If the
    /// media type is not `text`, or if it already has a `charset` parameter,
    /// then the media type will be left as-is.
    ///
    /// # Errors
    ///
    ///   * Mapped `io::Error`s from [`NamedFile::from_file()`].
    ///   * [`Error::InternalString`] if the constructed content-type cannot be
    ///     parsed (should never happen).
    fn into_named_file(self) -> Result<NamedFile> {
        let file = NamedFile::from_file(self.file, &self.path)?;
        let content_type = file.content_type();
        if content_type.type_() != mime::TEXT
            || content_type.params().any(|(name, _)| name == mime::CHARSET)
        {
            return Ok(file);
        }

        let new_content_type = format!("{}; charset=utf-8", &content_type);

        Ok(file.set_content_type(new_content_type.parse().map_err(
            |error| {
                Error::InternalString(format!(
                    "Parsing constructed content type {new_content_type:?}: \
                        {error}"
                ))
            },
        )?))
    }
}

impl Return for PathReturn {
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        _context: &'a Context<'a, V>,
    ) -> Result<ActionReturn> {
        Ok(self.into())
    }

    fn into_string_return(self) -> Result<StringReturn> {
        Ok(StringReturn::from(self.path))
    }

    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ContentReturn> {
        let Self { mut file, path, created, modified } = self;

        let mut body =
            String::with_capacity(file.metadata()?.len().try_into().map_err(
                |_| crate::Error::FileTooLarge(context.real_path(&path)),
            )?);

        #[expect(
            clippy::verbose_file_reads,
            reason = "required by code that checks if path is a file"
        )]
        file.read_to_string(&mut body)?;

        Ok(ContentReturn {
            body: body.into(),
            content_type: MediaType::APPLICATION_OCTET_STREAM,
            source: Source::File { path, modified, created },
            metadata: Metadata::new(),
        })
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse> {
        Ok(self
            .into_named_file()?
            .into_response(context.variables.request))
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
