//! UTF-8 source offsets, half-open locs, slicing, and line/column mapping.
//!
//! Offsets are byte-based and limited to PostgreSQL's signed 32-bit parse
//! `ParseLoc` values. [`SourceText`] validates boundaries before exposing source slices.

use std::fmt;
use std::ops::Add;
use std::ops::Sub;

/// A UTF-8 byte quantity used as a source offset, text length, or range shift.
///
/// Values are deliberately limited to `i32::MAX` so that source offsets remain
/// compatible with PostgreSQL's signed 32-bit `ParseLoc` values. Inputs that do
/// not fit are rejected instead of being truncated.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextSize(u32);

impl TextSize {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        assert!(value <= i32::MAX as u32, "text offset exceeds i32::MAX");
        Self(value)
    }

    /// Converts a validated UTF-8 byte quantity into a text size.
    ///
    /// # Panics
    ///
    /// Panics if `value` exceeds the supported text size range.
    #[track_caller]
    pub fn from_usize(value: usize) -> Self {
        match Self::try_from(value) {
            Ok(size) => size,
            Err(_) => panic!("text offset {value} exceeds i32::MAX"),
        }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<usize> for TextSize {
    type Error = SourceTooLarge;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        if value > i32::MAX as usize {
            return Err(SourceTooLarge { len: value });
        }
        Ok(Self(value as u32))
    }
}

impl From<TextSize> for usize {
    fn from(value: TextSize) -> Self {
        value.0 as usize
    }
}

impl Add for TextSize {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.0.checked_add(rhs.0).expect("text offset overflow"))
    }
}

impl Sub for TextSize {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.0.checked_sub(rhs.0).expect("text offset underflow"))
    }
}

/// A half-open offset in one SQL source: `[start, end)`.
///
/// Both endpoints are zero-based UTF-8 byte offsets. Construction validates
/// their ordering; [`SourceText`] validates their source bounds and UTF-8
/// character boundaries when the offset is resolved or sliced.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Loc {
    start: TextSize,
    end: TextSize,
}

impl Loc {
    pub fn new(start: TextSize, end: TextSize) -> Self {
        assert!(start <= end, "offset start must not exceed its end");
        Self { start, end }
    }

    pub const fn empty(offset: TextSize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    pub const fn start(self) -> TextSize {
        self.start
    }

    pub const fn end(self) -> TextSize {
        self.end
    }

    pub fn len(self) -> TextSize {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start.0 == self.end.0
    }

    pub fn contains(self, offset: TextSize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns the smallest offset containing both inputs.
    pub fn cover(left: Self, right: Self) -> Self {
        Self::new(left.start.min(right.start), left.end.max(right.end))
    }
}

impl Add<TextSize> for Loc {
    type Output = Self;

    fn add(self, offset: TextSize) -> Self {
        Self::new(self.start + offset, self.end + offset)
    }
}

impl Sub<TextSize> for Loc {
    type Output = Self;

    fn sub(self, offset: TextSize) -> Self {
        Self::new(self.start - offset, self.end - offset)
    }
}

/// A human-readable position in SQL source text.
///
/// Lines and columns are zero-based. Columns count Unicode scalar values;
/// protocol-specific encodings such as UTF-16 are adapter concerns.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Position {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based Unicode scalar-value column.
    pub column: u32,
}

/// The start and end positions corresponding to a [`Loc`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PositionRange {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceTooLarge {
    pub len: usize,
}

impl fmt::Display for SourceTooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SQL source is {} bytes; the maximum supported length is {} bytes",
            self.len,
            i32::MAX
        )
    }
}

impl std::error::Error for SourceTooLarge {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceError {
    TooLarge(SourceTooLarge),
    OutOfBounds { offset: TextSize, len: TextSize },
    NotCharBoundary { offset: TextSize },
    LineOutOfBounds { position: Position, line_count: u32 },
    ColumnOutOfBounds { position: Position, max_column: u32 },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(error) => error.fmt(f),
            Self::OutOfBounds { offset, len } => write!(
                f,
                "source offset {} is beyond source length {}",
                offset.get(),
                len.get()
            ),
            Self::NotCharBoundary { offset } => {
                write!(
                    f,
                    "source offset {} is not a UTF-8 character boundary",
                    offset.get()
                )
            }
            Self::LineOutOfBounds {
                position,
                line_count,
            } => write!(
                f,
                "source line {} is beyond the line count {}",
                position.line, line_count
            ),
            Self::ColumnOutOfBounds {
                position,
                max_column,
            } => write!(
                f,
                "source column {} is beyond line {} length {}",
                position.column, position.line, max_column
            ),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<SourceTooLarge> for SourceError {
    fn from(value: SourceTooLarge) -> Self {
        Self::TooLarge(value)
    }
}

/// Maps byte offsets to human-readable positions without storing line/column
/// pairs on every token or syntax node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineIndex<'a> {
    text: &'a str,
    len: TextSize,
    line_starts: Vec<TextSize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Result<Self, SourceTooLarge> {
        let len = TextSize::try_from(text.len())?;
        let mut line_starts = vec![TextSize::ZERO];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(TextSize::try_from(index + 1)?);
            }
        }
        Ok(Self {
            text,
            len,
            line_starts,
        })
    }

    pub fn position(&self, offset: TextSize) -> Result<Position, SourceError> {
        if offset > self.len {
            return Err(SourceError::OutOfBounds {
                offset,
                len: self.len,
            });
        }
        let offset_usize = usize::from(offset);
        if !self.text.is_char_boundary(offset_usize) {
            return Err(SourceError::NotCharBoundary { offset });
        }

        let line = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = usize::from(self.line_starts[line]);
        let content_end = self.line_content_end(line);
        let column = self.text[line_start..offset_usize.min(content_end)]
            .chars()
            .count();
        Ok(Position {
            line: line as u32,
            column: column as u32,
        })
    }

    /// Converts a zero-based Unicode-scalar position to a UTF-8 byte offset.
    pub fn offset(&self, position: Position) -> Result<TextSize, SourceError> {
        let line = usize::try_from(position.line).expect("u32 line fits usize");
        let Some(start) = self.line_starts.get(line).copied() else {
            return Err(SourceError::LineOutOfBounds {
                position,
                line_count: self.line_starts.len() as u32,
            });
        };
        let start = usize::from(start);
        let content_end = self.line_content_end(line);
        let content = &self.text[start..content_end];
        let column = usize::try_from(position.column).expect("u32 column fits usize");
        let offset = if column == content.chars().count() {
            content_end
        } else if let Some((relative, _)) = content.char_indices().nth(column) {
            start + relative
        } else {
            return Err(SourceError::ColumnOutOfBounds {
                position,
                max_column: content.chars().count() as u32,
            });
        };
        Ok(TextSize::from_usize(offset))
    }

    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    fn line_content_end(&self, line: usize) -> usize {
        let Some(next_start) = self.line_starts.get(line + 1).copied() else {
            return usize::from(self.len);
        };
        let mut end = usize::from(next_start);
        if self.text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
            if end > 0 && self.text.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
        }
        end
    }
}

/// SQL source text together with its reusable line index.
#[derive(Clone, Debug)]
pub struct SourceText<'a> {
    text: &'a str,
    line_index: LineIndex<'a>,
}

impl<'a> SourceText<'a> {
    pub fn new(text: &'a str) -> Result<Self, SourceTooLarge> {
        Ok(Self {
            text,
            line_index: LineIndex::new(text)?,
        })
    }

    pub const fn text(&self) -> &'a str {
        self.text
    }

    pub const fn len(&self) -> TextSize {
        self.line_index.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len().get() == 0
    }

    pub fn position(&self, offset: TextSize) -> Result<Position, SourceError> {
        self.line_index.position(offset)
    }

    pub fn offset(&self, position: Position) -> Result<TextSize, SourceError> {
        self.line_index.offset(position)
    }

    pub fn line_count(&self) -> u32 {
        self.line_index.line_count()
    }

    pub fn position_range(&self, loc: Loc) -> Result<PositionRange, SourceError> {
        Ok(PositionRange {
            start: self.position(loc.start())?,
            end: self.position(loc.end())?,
        })
    }

    pub fn slice(&self, loc: Loc) -> Result<&'a str, SourceError> {
        if loc.end() > self.len() {
            return Err(SourceError::OutOfBounds {
                offset: loc.end(),
                len: self.len(),
            });
        }
        let start = usize::from(loc.start());
        let end = usize::from(loc.end());
        if !self.text.is_char_boundary(start) {
            return Err(SourceError::NotCharBoundary {
                offset: loc.start(),
            });
        }
        if !self.text.is_char_boundary(end) {
            return Err(SourceError::NotCharBoundary { offset: loc.end() });
        }
        Ok(&self.text[start..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_utf8_and_crlf_positions() {
        let source = SourceText::new("中文\r\nfoo").unwrap();
        assert_eq!(
            source.position(TextSize::new(6)).unwrap(),
            Position { line: 0, column: 2 }
        );
        assert_eq!(
            source.position(TextSize::new(7)).unwrap(),
            Position { line: 0, column: 2 }
        );
        assert_eq!(
            source.position(TextSize::new(8)).unwrap(),
            Position { line: 1, column: 0 }
        );
        assert_eq!(source.line_count(), 2);
    }

    #[test]
    fn maps_positions_back_to_utf8_offsets() {
        let source = SourceText::new("中文\r\nfoo\n").unwrap();
        assert_eq!(
            source.offset(Position { line: 0, column: 2 }).unwrap(),
            TextSize::new(6)
        );
        assert_eq!(
            source.offset(Position { line: 1, column: 3 }).unwrap(),
            TextSize::new(11)
        );
        assert_eq!(
            source.offset(Position { line: 2, column: 0 }).unwrap(),
            TextSize::new(12)
        );
    }

    #[test]
    fn rejects_positions_outside_the_source() {
        let source = SourceText::new("中").unwrap();
        assert!(matches!(
            source.offset(Position { line: 1, column: 0 }),
            Err(SourceError::LineOutOfBounds { .. })
        ));
        assert!(matches!(
            source.offset(Position { line: 0, column: 2 }),
            Err(SourceError::ColumnOutOfBounds { .. })
        ));
    }

    #[test]
    fn rejects_offsets_inside_utf8_code_points() {
        let source = SourceText::new("中").unwrap();
        assert!(matches!(
            source.position(TextSize::new(1)),
            Err(SourceError::NotCharBoundary { .. })
        ));
    }

    #[test]
    fn slices_half_open_locs() {
        let source = SourceText::new("select 中文").unwrap();
        let loc = Loc::new(TextSize::new(7), TextSize::new(13));
        assert_eq!(source.slice(loc).unwrap(), "中文");
        assert_eq!(
            source.position_range(loc).unwrap(),
            PositionRange {
                start: Position { line: 0, column: 7 },
                end: Position { line: 0, column: 9 },
            }
        );
    }

    #[test]
    fn text_size_adds_and_subtracts() {
        let left = TextSize::new(10);
        let right = TextSize::new(3);
        assert_eq!(left + right, TextSize::new(13));
        assert_eq!(left - right, TextSize::new(7));
        assert_eq!(Loc::new(right, left).len(), TextSize::new(7));
    }

    #[test]
    fn constructs_text_size_from_usize() {
        assert_eq!(TextSize::from_usize(42), TextSize::new(42));
    }

    #[test]
    fn rejects_usize_outside_supported_range() {
        assert_eq!(
            TextSize::try_from(i32::MAX as usize + 1),
            Err(SourceTooLarge {
                len: i32::MAX as usize + 1,
            })
        );
    }

    #[test]
    fn loc_shifts_by_offset() {
        let loc = Loc::new(TextSize::new(3), TextSize::new(10));
        let base = TextSize::new(100);
        assert_eq!(loc + base, Loc::new(TextSize::new(103), TextSize::new(110)));
        assert_eq!((loc + base) - base, loc);
        assert_eq!(
            Loc::cover(Loc::new(TextSize::new(5), TextSize::new(8)), loc),
            Loc::new(TextSize::new(3), TextSize::new(10))
        );
    }
}
