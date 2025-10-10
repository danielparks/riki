//! Filter ASCII characters against a bit filter.
//!
//! Used to quickly determine if characters in a string are an valid.
//!
//! ```
//! use riki::config::bitfilter::BitFilter;
//!
//! const UPPER: BitFilter = BitFilter::from_str("A-Z");
//!
//! fn main() {
//!     assert!(UPPER.match_char('N'));
//!     assert!(!UPPER.match_char('n'));
//!     assert!(!UPPER.match_char(' '));
//!     assert!(!UPPER.match_char('π'));
//! }
//! ```

use std::error;
use std::fmt;

/// The bit filter
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitFilter(pub u128);

impl BitFilter {
    /// Create a `BitFilter` from a range string.
    ///
    /// ```
    /// use riki::config::bitfilter::{BitFilter, Error};
    ///
    /// assert_eq!(
    ///     Ok(BitFilter(1 << b'A')),
    ///     BitFilter::try_from_bytes(b"A"),
    /// );
    /// assert_eq!(
    ///     Err(Error::NonAscii { index: 0 }),
    ///     BitFilter::try_from_bytes("é".as_bytes()),
    /// );
    /// assert_eq!(
    ///     Err(Error::InvalidRange { index: 1 }),
    ///     BitFilter::try_from_bytes(b" z-a"),
    /// );
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for invalid range strings.
    #[expect(clippy::arithmetic_side_effects, reason = "checks first")]
    pub const fn try_from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut bits = 0u128;
        if bytes.is_empty() {
            return Ok(Self(bits));
        }

        // First byte; can be b'-'
        let mut i = 0;
        if bytes[i] > 127 {
            return Err(Error::NonAscii { index: i });
        }
        bits |= 1 << bytes[i];
        i += 1;

        if i >= bytes.len() {
            return Ok(Self(bits));
        }
        while i < bytes.len() - 1 {
            let b = bytes[i];
            if b > 127 {
                return Err(Error::NonAscii { index: i });
            }

            if b == b'-' {
                // Found range
                if bytes[i - 1] > bytes[i + 1] {
                    return Err(Error::InvalidRange { index: i - 1 });
                }
                let mut b = bytes[i - 1] + 1; // Already added the start byte
                while b < bytes[i + 1] {
                    // Will add the end byte next loop
                    bits |= 1 << b;
                    b += 1;
                }
            } else {
                bits |= 1 << b;
            }

            i += 1;
        }

        // Last byte; can be b'-'
        if bytes[i] > 127 {
            return Err(Error::NonAscii { index: i });
        }
        bits |= 1 << bytes[i];

        Ok(Self(bits))
    }

    /// Create a `BitFilter` from a range string, or panic.
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// const UPPER: BitFilter = BitFilter::from_bytes(b"A-Z");
    ///
    /// fn main() {
    ///     assert!(UPPER.match_byte(b'N'));
    ///     assert!(!UPPER.match_byte(b'n'));
    ///     assert!(!UPPER.match_byte(b' '));
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the bytes contains non-ASCII or an invalid range.
    #[must_use]
    #[inline]
    pub const fn from_bytes(bytes: &[u8]) -> Self {
        match Self::try_from_bytes(bytes) {
            Ok(filter) => filter,
            Err(error) => error.panic(),
        }
    }

    /// Create a `BitFilter` from a range string.
    ///
    /// ```
    /// use riki::config::bitfilter::{BitFilter, Error};
    ///
    /// assert_eq!(
    ///     Ok(BitFilter(1 << b'A')),
    ///     BitFilter::try_from_str("A"),
    /// );
    /// assert_eq!(
    ///     Err(Error::NonAscii { index: 0 }),
    ///     BitFilter::try_from_str("é"),
    /// );
    /// assert_eq!(
    ///     Err(Error::InvalidRange { index: 1 }),
    ///     BitFilter::try_from_str(" z-a"),
    /// );
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for invalid range strings or non-ASCII.
    #[inline]
    pub const fn try_from_str(chars: &str) -> Result<Self, Error> {
        Self::try_from_bytes(chars.as_bytes())
    }

    /// Create a `BitFilter` from a range string, or panic.
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// const UPPER: BitFilter = BitFilter::from_str("A-Z");
    ///
    /// fn main() {
    ///     assert!(UPPER.match_byte(b'N'));
    ///     assert!(!UPPER.match_byte(b'n'));
    ///     assert!(!UPPER.match_byte(b' '));
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the string contains non-ASCII or an invalid range.
    #[must_use]
    #[inline]
    pub const fn from_str(chars: &str) -> Self {
        Self::from_bytes(chars.as_bytes())
    }

    /// Create a `BitFilter` from a range string, or panic.
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// const UPPER: BitFilter = BitFilter::from_bracketed_str("[A-Z]");
    ///
    /// fn main() {
    ///     assert!(UPPER.match_byte(b'N'));
    ///     assert!(!UPPER.match_byte(b'n'));
    ///     assert!(!UPPER.match_byte(b'['));
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the string isn’t wrapped in "[...]", or if it contains
    /// non-ASCII, or if it has an invalid range.
    #[expect(clippy::arithmetic_side_effects, reason = "checks first")]
    #[must_use]
    #[inline]
    pub const fn from_bracketed_str(chars: &str) -> Self {
        assert!(chars.len() >= 2, "must start with '[' and end with ']'");
        let (start, bytes) = chars.as_bytes().split_at(1);
        let (bytes, end) = bytes.split_at(bytes.len() - 1);
        assert!(start.len() == 1 && start[0] == b'[', "must start with '['");
        assert!(end.len() == 1 && end[0] == b']', "must end with ']'");
        Self::from_bytes(bytes)
    }

    /// Invert `BitFilter`.
    ///
    /// Note this still won’t match non-ASCII bytes. See
    /// [`Self::match_or_non_ascii()`].
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// let upper = BitFilter::from_bytes(b"A-Z");
    ///
    /// assert!(upper.match_char('N'));
    /// assert!(!upper.match_char('n'));
    /// assert!(!upper.match_char(' '));
    /// assert!(!upper.match_char('π'));
    ///
    /// let non_upper = upper.inverted();
    ///
    /// assert!(!non_upper.match_char('N'));
    /// assert!(non_upper.match_char('n'));
    /// assert!(non_upper.match_char(' '));
    /// assert!(!non_upper.match_char('π'));
    /// ```
    #[must_use]
    #[inline]
    pub const fn inverted(&self) -> Self {
        Self(!self.0)
    }

    /// Add a matching byte.
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// let mut filter = BitFilter::from_bytes(b"0-9");
    ///
    /// assert!(filter.match_char('1'));
    /// assert!(!filter.match_char('a'));
    ///
    /// filter.add(b'a').unwrap();
    ///
    /// assert!(filter.match_char('1'));
    /// assert!(filter.match_char('a'));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::NonAscii`] at index 0 if the passed byte is not ASCII.
    #[inline]
    pub const fn add(&mut self, b: u8) -> Result<&mut Self, Error> {
        if b > 127 {
            return Err(Error::NonAscii { index: 0 });
        }
        self.0 |= 1 << b;
        Ok(self)
    }

    /// match if a byte matches the filter
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// let UPPER = BitFilter::from_bytes(b"A-Z");
    /// assert!(UPPER.match_byte(b'N'));
    /// assert!(!UPPER.match_byte(b'n'));
    /// assert!(!UPPER.match_byte(b' '));
    /// assert!(!UPPER.match_byte(128));
    /// ```
    #[must_use]
    #[inline]
    pub const fn match_byte(self, b: u8) -> bool {
        b <= 127 && self.0 & (1 << b) != 0
    }

    /// match if a char matches the filter
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// let UPPER = BitFilter::from_bytes(b"A-Z");
    /// assert!(UPPER.match_char('N'));
    /// assert!(!UPPER.match_char('n'));
    /// assert!(!UPPER.match_char(' '));
    /// assert!(!UPPER.match_char('π'));
    /// ```
    #[must_use]
    #[inline]
    pub const fn match_char(self, c: char) -> bool {
        c <= 127 as char && self.match_byte(c as u8)
    }

    /// match if a char matches the filter, or isn’t ASCII
    ///
    /// ```
    /// use riki::config::bitfilter::BitFilter;
    ///
    /// let UPPER = BitFilter::from_bytes(b"A-Z");
    /// assert!(UPPER.match_or_non_ascii('N'));
    /// assert!(!UPPER.match_or_non_ascii('n'));
    /// assert!(!UPPER.match_or_non_ascii(' '));
    /// assert!(UPPER.match_or_non_ascii('π'));
    /// ```
    #[must_use]
    #[inline]
    pub const fn match_or_non_ascii(self, c: char) -> bool {
        c > 127 as char || self.match_byte(c as u8)
    }
}

/// An error when creating a bit filter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Found non-ASCII byte.
    NonAscii {
        /// The index of the invalid byte.
        index: usize,
    },
    /// Invalid range: start is greater than end
    InvalidRange {
        /// The index of the start of the invalid range.
        index: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonAscii { index } => {
                write!(f, "found non-ASCII byte at {index}")
            }
            Self::InvalidRange { index } => {
                write!(f, "found range with start greater than end at {index}")
            }
        }
    }
}

impl error::Error for Error {}

impl Error {
    /// Panic, displaying the basic error message.
    ///
    /// # Panics
    ///
    /// Always.
    pub const fn panic(&self) -> ! {
        match self {
            Self::NonAscii { .. } => panic!("found non-ASCII byte"),
            Self::InvalidRange { .. } => {
                panic!("found range with start greater than end ")
            }
        }
    }
}
