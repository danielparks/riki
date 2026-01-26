//! Actions to generate an process data from the web server.
//!
//! ## Returns
//!
//! Types that implement [`Return`] can be passed from action to action for
//! processing then converted into an [`actix_web::HttpResponse`] for return to
//! the client.
//!
//!   * A [`PathReturn`] represents a path of the firesystem that is guaranteed
//!     to be a file — it opens the file and holds its descriptor.
//!   * A [`ContentReturn`] holds actual content, possibly with a path and other
//!     metadata attached to it. It does not hold open a file descriptor.
//!
//! ### Very large files
//!
//! A [`PathReturn`] can be transformed into a [`actix_files::NamedFile`], which
//! Actix can stream back to the client.
//!
//! However, to do any transformations, e.g. converting Markdown to HTML or
//! rendering HTML into a template, the entire file must be loaded into memory
//! as a [`ContentReturn`].

mod ret;
pub use ret::*;
