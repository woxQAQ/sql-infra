use std::fmt;
use std::ops::{Add, Sub};

/// A UTF-8 byte offset into a SQL source string.
///
/// Values are deliberately limited to `i32::MAX`: PostgreSQL parse locations
/// use signed 32-bit integers. Inputs that do not fit are rejected instead of
/// being truncated.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextSize(u32);

impl TextSize {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        assert!(value <= i32::MAX as u32, "text offset exceeds i32::MAX");
        Self(value)
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
        Self::new(
            self.0
                .checked_add(rhs.0)
                .expect("text offset overflow"),
        )
    }
}

impl Sub for TextSize {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(
            self.0
                .checked_sub(rhs.0)
                .expect("text offset underflow"),
        )
    }
}

/// A half-open UTF-8 byte range: `[start, end)`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TextRange {
    start: TextSize,
    end: TextSize,
}

impl TextRange {
    pub fn new(start: TextSize, end: TextSize) -> Self {
        assert!(start <= end, "text range start must not exceed its end");
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
}

impl Add<TextSize> for TextRange {
    type Output = Self;

    fn add(self, offset: TextSize) -> Self {
        Self::new(self.start + offset, self.end + offset)
    }
}

impl Sub<TextSize> for TextRange {
    type Output = Self;

    fn sub(self, offset: TextSize) -> Self {
        Self::new(self.start - offset, self.end - offset)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineColumn {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based Unicode scalar-value column.
    pub column: u32,
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
    line_starts: Vec<TextSize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Result<Self, SourceTooLarge> {
        TextSize::try_from(text.len())?;
        let mut line_starts = vec![TextSize::ZERO];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(TextSize::try_from(index + 1)?);
            }
        }
        Ok(Self { text, line_starts })
    }

    pub fn line_column(&self, offset: TextSize) -> Result<LineColumn, SourceError> {
        let len = TextSize::try_from(self.text.len())?;
        if offset > len {
            return Err(SourceError::OutOfBounds { offset, len });
        }
        let offset_usize = usize::from(offset);
        if !self.text.is_char_boundary(offset_usize) {
            return Err(SourceError::NotCharBoundary { offset });
        }

        let line = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = usize::from(self.line_starts[line]);
        let column = self.text[line_start..offset_usize].chars().count();
        Ok(LineColumn {
            line: line as u32,
            column: column as u32,
        })
    }
}

/// SQL source text together with its reusable line index.
#[derive(Clone, Debug)]
pub struct SourceText<'a> {
    text: &'a str,
    len: TextSize,
    line_index: LineIndex<'a>,
}

impl<'a> SourceText<'a> {
    pub fn new(text: &'a str) -> Result<Self, SourceTooLarge> {
        Ok(Self {
            text,
            len: TextSize::try_from(text.len())?,
            line_index: LineIndex::new(text)?,
        })
    }

    pub const fn text(&self) -> &'a str {
        self.text
    }

    pub const fn len(&self) -> TextSize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len.get() == 0
    }

    pub fn line_column(&self, offset: TextSize) -> Result<LineColumn, SourceError> {
        self.line_index.line_column(offset)
    }

    pub fn range_line_columns(
        &self,
        range: TextRange,
    ) -> Result<(LineColumn, LineColumn), SourceError> {
        Ok((
            self.line_column(range.start())?,
            self.line_column(range.end())?,
        ))
    }

    pub fn slice(&self, range: TextRange) -> Result<&'a str, SourceError> {
        if range.end() > self.len {
            return Err(SourceError::OutOfBounds {
                offset: range.end(),
                len: self.len,
            });
        }
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        if !self.text.is_char_boundary(start) {
            return Err(SourceError::NotCharBoundary {
                offset: range.start(),
            });
        }
        if !self.text.is_char_boundary(end) {
            return Err(SourceError::NotCharBoundary {
                offset: range.end(),
            });
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
            source.line_column(TextSize::new(6)).unwrap(),
            LineColumn { line: 0, column: 2 }
        );
        assert_eq!(
            source.line_column(TextSize::new(8)).unwrap(),
            LineColumn { line: 1, column: 0 }
        );
    }

    #[test]
    fn rejects_offsets_inside_utf8_code_points() {
        let source = SourceText::new("中").unwrap();
        assert!(matches!(
            source.line_column(TextSize::new(1)),
            Err(SourceError::NotCharBoundary { .. })
        ));
    }

    #[test]
    fn slices_half_open_ranges() {
        let source = SourceText::new("select 中文").unwrap();
        let range = TextRange::new(TextSize::new(7), TextSize::new(13));
        assert_eq!(source.slice(range).unwrap(), "中文");
    }

    #[test]
    fn text_size_adds_and_subtracts() {
        let left = TextSize::new(10);
        let right = TextSize::new(3);
        assert_eq!(left + right, TextSize::new(13));
        assert_eq!(left - right, TextSize::new(7));
        assert_eq!(TextRange::new(right, left).len(), TextSize::new(7));
    }

    #[test]
    fn text_range_shifts_by_offset() {
        let range = TextRange::new(TextSize::new(3), TextSize::new(10));
        let base = TextSize::new(100);
        assert_eq!(range + base, TextRange::new(TextSize::new(103), TextSize::new(110)));
        assert_eq!(
            (range + base) - base,
            range
        );
    }
}
