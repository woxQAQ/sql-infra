// Translated by hand from PostgreSQL's src/backend/parser/scan.l semantics.
// Token names come from gram.y and keyword mappings from parser/kwlist.h.

const NAMEDATALEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TokenKind {
    Eof,
    Char(char),
    Ident,
    UIdent,
    FConst,
    SConst,
    USConst,
    BConst,
    XConst,
    Op,
    IConst,
    Param,
    TypeCast,
    DotDot,
    ColonEquals,
    EqualsGreater,
    LessEquals,
    GreaterEquals,
    NotEquals,
    AbortP,
    Absent,
    AbsoluteP,
    Access,
    Action,
    AddP,
    Admin,
    After,
    Aggregate,
    All,
    Also,
    Alter,
    Always,
    Analyse,
    Analyze,
    And,
    Any,
    Array,
    As,
    Asc,
    Asensitive,
    Assertion,
    Assignment,
    Asymmetric,
    Atomic,
    At,
    Attach,
    Attribute,
    Authorization,
    Backward,
    Before,
    BeginP,
    Between,
    Bigint,
    Binary,
    Bit,
    BooleanP,
    Both,
    Breadth,
    By,
    Cache,
    Call,
    Called,
    Cascade,
    Cascaded,
    Case,
    Cast,
    CatalogP,
    Chain,
    CharP,
    Character,
    Characteristics,
    Check,
    Checkpoint,
    Class,
    Close,
    Cluster,
    Coalesce,
    Collate,
    Collation,
    Column,
    Columns,
    Comment,
    Comments,
    Commit,
    Committed,
    Compression,
    Concurrently,
    Conditional,
    Configuration,
    Conflict,
    Connection,
    Constraint,
    Constraints,
    ContentP,
    ContinueP,
    ConversionP,
    Copy,
    Cost,
    Create,
    Cross,
    Csv,
    Cube,
    CurrentP,
    CurrentCatalog,
    CurrentDate,
    CurrentRole,
    CurrentSchema,
    CurrentTime,
    CurrentTimestamp,
    CurrentUser,
    Cursor,
    Cycle,
    DataP,
    Database,
    DayP,
    Deallocate,
    Dec,
    DecimalP,
    Declare,
    Default,
    Defaults,
    Deferrable,
    Deferred,
    Definer,
    DeleteP,
    Delimiter,
    Delimiters,
    Depends,
    Depth,
    Desc,
    Destination,
    Detach,
    Dictionary,
    DisableP,
    Discard,
    Distinct,
    Do,
    DocumentP,
    DomainP,
    DoubleP,
    Drop,
    Each,
    Edge,
    Else,
    EmptyP,
    EnableP,
    Encoding,
    Encrypted,
    EndP,
    Enforced,
    EnumP,
    ErrorP,
    Escape,
    Event,
    Except,
    Exclude,
    Excluding,
    Exclusive,
    Execute,
    Exists,
    Explain,
    Expression,
    Extension,
    External,
    Extract,
    FalseP,
    Family,
    Fetch,
    Filter,
    Finalize,
    FirstP,
    FloatP,
    Following,
    For,
    Force,
    Foreign,
    Format,
    Forward,
    Freeze,
    From,
    Full,
    Function,
    Functions,
    Generated,
    Global,
    Grant,
    Granted,
    Graph,
    GraphTable,
    Greatest,
    GroupP,
    Grouping,
    Groups,
    Handler,
    Having,
    HeaderP,
    Hold,
    HourP,
    IdentityP,
    IfP,
    IgnoreP,
    Ilike,
    Immediate,
    Immutable,
    ImplicitP,
    ImportP,
    InP,
    Include,
    Including,
    Increment,
    Indent,
    Index,
    Indexes,
    Inherit,
    Inherits,
    Initially,
    InlineP,
    InnerP,
    Inout,
    InputP,
    Insensitive,
    Insert,
    Instead,
    IntP,
    Integer,
    Intersect,
    Interval,
    Into,
    Invoker,
    Is,
    Isnull,
    Isolation,
    Join,
    Json,
    JsonArray,
    JsonArrayagg,
    JsonExists,
    JsonObject,
    JsonObjectagg,
    JsonQuery,
    JsonScalar,
    JsonSerialize,
    JsonTable,
    JsonValue,
    Keep,
    Key,
    Keys,
    Label,
    Language,
    LargeP,
    LastP,
    LateralP,
    Leading,
    Leakproof,
    Least,
    Left,
    Level,
    Like,
    Limit,
    Listen,
    Load,
    Local,
    Localtime,
    Localtimestamp,
    Location,
    LockP,
    Locked,
    Logged,
    LsnP,
    Mapping,
    Match,
    Matched,
    Materialized,
    Maxvalue,
    Merge,
    MergeAction,
    Method,
    MinuteP,
    Minvalue,
    Mode,
    MonthP,
    Move,
    NameP,
    Names,
    National,
    Natural,
    Nchar,
    Nested,
    New,
    Next,
    Nfc,
    Nfd,
    Nfkc,
    Nfkd,
    No,
    Node,
    None,
    Normalize,
    Normalized,
    Not,
    Nothing,
    Notify,
    Notnull,
    Nowait,
    NullP,
    Nullif,
    NullsP,
    Numeric,
    ObjectP,
    ObjectsP,
    Of,
    Off,
    Offset,
    Oids,
    Old,
    Omit,
    On,
    Only,
    Operator,
    Option,
    Options,
    Or,
    Order,
    Ordinality,
    Others,
    OutP,
    OuterP,
    Over,
    Overlaps,
    Overlay,
    Overriding,
    Owned,
    Owner,
    Parallel,
    Parameter,
    Parser,
    Partial,
    Partition,
    Partitions,
    Passing,
    Password,
    Path,
    Period,
    Placing,
    Plan,
    Plans,
    Policy,
    Portion,
    Position,
    Preceding,
    Precision,
    Preserve,
    Prepare,
    Prepared,
    Primary,
    Prior,
    Privileges,
    Procedural,
    Procedure,
    Procedures,
    Program,
    Properties,
    Property,
    Publication,
    Quote,
    Quotes,
    Range,
    Read,
    Real,
    Reassign,
    Recursive,
    RefP,
    References,
    Referencing,
    Refresh,
    Reindex,
    Relationship,
    RelativeP,
    Release,
    Rename,
    Repack,
    Repeatable,
    Replace,
    Replica,
    Reset,
    RespectP,
    Restart,
    Restrict,
    Return,
    Returning,
    Returns,
    Revoke,
    Right,
    Role,
    Rollback,
    Rollup,
    Routine,
    Routines,
    Row,
    Rows,
    Rule,
    Savepoint,
    Scalar,
    Schema,
    Schemas,
    Scroll,
    Search,
    SecondP,
    Security,
    Select,
    Sequence,
    Sequences,
    Serializable,
    Server,
    Session,
    SessionUser,
    Set,
    Sets,
    Setof,
    Share,
    Show,
    Similar,
    Simple,
    Skip,
    Smallint,
    Snapshot,
    Some,
    Split,
    Source,
    SqlP,
    Stable,
    StandaloneP,
    Start,
    Statement,
    Statistics,
    Stdin,
    Stdout,
    Storage,
    Stored,
    StrictP,
    StringP,
    StripP,
    Subscription,
    Substring,
    Support,
    Symmetric,
    Sysid,
    SystemP,
    SystemUser,
    Table,
    Tables,
    Tablesample,
    Tablespace,
    Target,
    Temp,
    Template,
    Temporary,
    TextP,
    Then,
    Ties,
    Time,
    Timestamp,
    To,
    Trailing,
    Transaction,
    Transform,
    Treat,
    Trigger,
    Trim,
    TrueP,
    Truncate,
    Trusted,
    TypeP,
    TypesP,
    Uescape,
    Unbounded,
    Unconditional,
    Uncommitted,
    Unencrypted,
    Union,
    Unique,
    Unknown,
    Unlisten,
    Unlogged,
    Until,
    Update,
    User,
    Using,
    Vacuum,
    Valid,
    Validate,
    Validator,
    ValueP,
    Values,
    Varchar,
    Variadic,
    Varying,
    Verbose,
    VersionP,
    Vertex,
    View,
    Views,
    Virtual,
    Volatile,
    Wait,
    When,
    Where,
    WhitespaceP,
    Window,
    With,
    Within,
    Without,
    Work,
    Wrapper,
    Write,
    XmlP,
    Xmlattributes,
    Xmlconcat,
    Xmlelement,
    Xmlexists,
    Xmlforest,
    Xmlnamespaces,
    Xmlparse,
    Xmlpi,
    Xmlroot,
    Xmlserialize,
    Xmltable,
    YearP,
    YesP,
    Zone,
    FormatLa,
    NotLa,
    NullsLa,
    WithLa,
    WithoutLa,
    ModeTypeName,
    ModePlpgsqlExpr,
    ModePlpgsqlAssign1,
    ModePlpgsqlAssign2,
    ModePlpgsqlAssign3,
    RightArrow,
    Uminus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum KeywordCategory {
    Unreserved,
    ColName,
    TypeFuncName,
    Reserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum BareLabel {
    Bare,
    As,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Keyword {
    pub word: &'static str,
    pub kind: TokenKind,
    pub category: KeywordCategory,
    pub bare_label: BareLabel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenValue {
    Integer(i32),
    String(std::string::String),
    Keyword(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub location: usize,
    pub value: Option<TokenValue>,
}

impl Token {
    fn new(kind: TokenKind, location: usize) -> Self {
        Self {
            kind,
            location,
            value: None,
        }
    }

    fn string(kind: TokenKind, location: usize, value: impl Into<std::string::String>) -> Self {
        Self {
            kind,
            location,
            value: Some(TokenValue::String(value.into())),
        }
    }

    fn integer(kind: TokenKind, location: usize, value: i32) -> Self {
        Self {
            kind,
            location,
            value: Some(TokenValue::Integer(value)),
        }
    }

    fn keyword(kind: TokenKind, location: usize, word: &'static str) -> Self {
        Self {
            kind,
            location,
            value: Some(TokenValue::Keyword(word)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexError {
    pub message: std::string::String,
    pub location: usize,
}

impl LexError {
    fn new(location: usize, message: impl Into<std::string::String>) -> Self {
        Self {
            location,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.location)
    }
}

impl std::error::Error for LexError {}

pub static KEYWORDS: &[Keyword] = &[
    Keyword {
        word: "abort",
        kind: TokenKind::AbortP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "absent",
        kind: TokenKind::Absent,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "absolute",
        kind: TokenKind::AbsoluteP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "access",
        kind: TokenKind::Access,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "action",
        kind: TokenKind::Action,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "add",
        kind: TokenKind::AddP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "admin",
        kind: TokenKind::Admin,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "after",
        kind: TokenKind::After,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "aggregate",
        kind: TokenKind::Aggregate,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "all",
        kind: TokenKind::All,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "also",
        kind: TokenKind::Also,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "alter",
        kind: TokenKind::Alter,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "always",
        kind: TokenKind::Always,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "analyse",
        kind: TokenKind::Analyse,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "analyze",
        kind: TokenKind::Analyze,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "and",
        kind: TokenKind::And,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "any",
        kind: TokenKind::Any,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "array",
        kind: TokenKind::Array,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "as",
        kind: TokenKind::As,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "asc",
        kind: TokenKind::Asc,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "asensitive",
        kind: TokenKind::Asensitive,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "assertion",
        kind: TokenKind::Assertion,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "assignment",
        kind: TokenKind::Assignment,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "asymmetric",
        kind: TokenKind::Asymmetric,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "at",
        kind: TokenKind::At,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "atomic",
        kind: TokenKind::Atomic,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "attach",
        kind: TokenKind::Attach,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "attribute",
        kind: TokenKind::Attribute,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "authorization",
        kind: TokenKind::Authorization,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "backward",
        kind: TokenKind::Backward,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "before",
        kind: TokenKind::Before,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "begin",
        kind: TokenKind::BeginP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "between",
        kind: TokenKind::Between,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "bigint",
        kind: TokenKind::Bigint,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "binary",
        kind: TokenKind::Binary,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "bit",
        kind: TokenKind::Bit,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "boolean",
        kind: TokenKind::BooleanP,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "both",
        kind: TokenKind::Both,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "breadth",
        kind: TokenKind::Breadth,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "by",
        kind: TokenKind::By,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cache",
        kind: TokenKind::Cache,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "call",
        kind: TokenKind::Call,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "called",
        kind: TokenKind::Called,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cascade",
        kind: TokenKind::Cascade,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cascaded",
        kind: TokenKind::Cascaded,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "case",
        kind: TokenKind::Case,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cast",
        kind: TokenKind::Cast,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "catalog",
        kind: TokenKind::CatalogP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "chain",
        kind: TokenKind::Chain,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "char",
        kind: TokenKind::CharP,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "character",
        kind: TokenKind::Character,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "characteristics",
        kind: TokenKind::Characteristics,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "check",
        kind: TokenKind::Check,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "checkpoint",
        kind: TokenKind::Checkpoint,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "class",
        kind: TokenKind::Class,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "close",
        kind: TokenKind::Close,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cluster",
        kind: TokenKind::Cluster,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "coalesce",
        kind: TokenKind::Coalesce,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "collate",
        kind: TokenKind::Collate,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "collation",
        kind: TokenKind::Collation,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "column",
        kind: TokenKind::Column,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "columns",
        kind: TokenKind::Columns,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "comment",
        kind: TokenKind::Comment,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "comments",
        kind: TokenKind::Comments,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "commit",
        kind: TokenKind::Commit,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "committed",
        kind: TokenKind::Committed,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "compression",
        kind: TokenKind::Compression,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "concurrently",
        kind: TokenKind::Concurrently,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "conditional",
        kind: TokenKind::Conditional,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "configuration",
        kind: TokenKind::Configuration,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "conflict",
        kind: TokenKind::Conflict,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "connection",
        kind: TokenKind::Connection,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "constraint",
        kind: TokenKind::Constraint,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "constraints",
        kind: TokenKind::Constraints,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "content",
        kind: TokenKind::ContentP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "continue",
        kind: TokenKind::ContinueP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "conversion",
        kind: TokenKind::ConversionP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "copy",
        kind: TokenKind::Copy,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cost",
        kind: TokenKind::Cost,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "create",
        kind: TokenKind::Create,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "cross",
        kind: TokenKind::Cross,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "csv",
        kind: TokenKind::Csv,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cube",
        kind: TokenKind::Cube,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current",
        kind: TokenKind::CurrentP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current_catalog",
        kind: TokenKind::CurrentCatalog,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current_date",
        kind: TokenKind::CurrentDate,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current_role",
        kind: TokenKind::CurrentRole,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current_schema",
        kind: TokenKind::CurrentSchema,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current_time",
        kind: TokenKind::CurrentTime,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current_timestamp",
        kind: TokenKind::CurrentTimestamp,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "current_user",
        kind: TokenKind::CurrentUser,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cursor",
        kind: TokenKind::Cursor,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "cycle",
        kind: TokenKind::Cycle,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "data",
        kind: TokenKind::DataP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "database",
        kind: TokenKind::Database,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "day",
        kind: TokenKind::DayP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "deallocate",
        kind: TokenKind::Deallocate,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "dec",
        kind: TokenKind::Dec,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "decimal",
        kind: TokenKind::DecimalP,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "declare",
        kind: TokenKind::Declare,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "default",
        kind: TokenKind::Default,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "defaults",
        kind: TokenKind::Defaults,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "deferrable",
        kind: TokenKind::Deferrable,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "deferred",
        kind: TokenKind::Deferred,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "definer",
        kind: TokenKind::Definer,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "delete",
        kind: TokenKind::DeleteP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "delimiter",
        kind: TokenKind::Delimiter,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "delimiters",
        kind: TokenKind::Delimiters,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "depends",
        kind: TokenKind::Depends,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "depth",
        kind: TokenKind::Depth,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "desc",
        kind: TokenKind::Desc,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "destination",
        kind: TokenKind::Destination,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "detach",
        kind: TokenKind::Detach,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "dictionary",
        kind: TokenKind::Dictionary,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "disable",
        kind: TokenKind::DisableP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "discard",
        kind: TokenKind::Discard,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "distinct",
        kind: TokenKind::Distinct,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "do",
        kind: TokenKind::Do,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "document",
        kind: TokenKind::DocumentP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "domain",
        kind: TokenKind::DomainP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "double",
        kind: TokenKind::DoubleP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "drop",
        kind: TokenKind::Drop,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "each",
        kind: TokenKind::Each,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "edge",
        kind: TokenKind::Edge,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "else",
        kind: TokenKind::Else,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "empty",
        kind: TokenKind::EmptyP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "enable",
        kind: TokenKind::EnableP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "encoding",
        kind: TokenKind::Encoding,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "encrypted",
        kind: TokenKind::Encrypted,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "end",
        kind: TokenKind::EndP,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "enforced",
        kind: TokenKind::Enforced,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "enum",
        kind: TokenKind::EnumP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "error",
        kind: TokenKind::ErrorP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "escape",
        kind: TokenKind::Escape,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "event",
        kind: TokenKind::Event,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "except",
        kind: TokenKind::Except,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "exclude",
        kind: TokenKind::Exclude,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "excluding",
        kind: TokenKind::Excluding,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "exclusive",
        kind: TokenKind::Exclusive,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "execute",
        kind: TokenKind::Execute,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "exists",
        kind: TokenKind::Exists,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "explain",
        kind: TokenKind::Explain,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "expression",
        kind: TokenKind::Expression,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "extension",
        kind: TokenKind::Extension,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "external",
        kind: TokenKind::External,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "extract",
        kind: TokenKind::Extract,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "false",
        kind: TokenKind::FalseP,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "family",
        kind: TokenKind::Family,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "fetch",
        kind: TokenKind::Fetch,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "filter",
        kind: TokenKind::Filter,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "finalize",
        kind: TokenKind::Finalize,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "first",
        kind: TokenKind::FirstP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "float",
        kind: TokenKind::FloatP,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "following",
        kind: TokenKind::Following,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "for",
        kind: TokenKind::For,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "force",
        kind: TokenKind::Force,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "foreign",
        kind: TokenKind::Foreign,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "format",
        kind: TokenKind::Format,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "forward",
        kind: TokenKind::Forward,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "freeze",
        kind: TokenKind::Freeze,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "from",
        kind: TokenKind::From,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "full",
        kind: TokenKind::Full,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "function",
        kind: TokenKind::Function,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "functions",
        kind: TokenKind::Functions,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "generated",
        kind: TokenKind::Generated,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "global",
        kind: TokenKind::Global,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "grant",
        kind: TokenKind::Grant,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "granted",
        kind: TokenKind::Granted,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "graph",
        kind: TokenKind::Graph,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "graph_table",
        kind: TokenKind::GraphTable,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "greatest",
        kind: TokenKind::Greatest,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "group",
        kind: TokenKind::GroupP,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "grouping",
        kind: TokenKind::Grouping,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "groups",
        kind: TokenKind::Groups,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "handler",
        kind: TokenKind::Handler,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "having",
        kind: TokenKind::Having,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "header",
        kind: TokenKind::HeaderP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "hold",
        kind: TokenKind::Hold,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "hour",
        kind: TokenKind::HourP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "identity",
        kind: TokenKind::IdentityP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "if",
        kind: TokenKind::IfP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "ignore",
        kind: TokenKind::IgnoreP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "ilike",
        kind: TokenKind::Ilike,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "immediate",
        kind: TokenKind::Immediate,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "immutable",
        kind: TokenKind::Immutable,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "implicit",
        kind: TokenKind::ImplicitP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "import",
        kind: TokenKind::ImportP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "in",
        kind: TokenKind::InP,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "include",
        kind: TokenKind::Include,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "including",
        kind: TokenKind::Including,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "increment",
        kind: TokenKind::Increment,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "indent",
        kind: TokenKind::Indent,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "index",
        kind: TokenKind::Index,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "indexes",
        kind: TokenKind::Indexes,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "inherit",
        kind: TokenKind::Inherit,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "inherits",
        kind: TokenKind::Inherits,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "initially",
        kind: TokenKind::Initially,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "inline",
        kind: TokenKind::InlineP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "inner",
        kind: TokenKind::InnerP,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "inout",
        kind: TokenKind::Inout,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "input",
        kind: TokenKind::InputP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "insensitive",
        kind: TokenKind::Insensitive,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "insert",
        kind: TokenKind::Insert,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "instead",
        kind: TokenKind::Instead,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "int",
        kind: TokenKind::IntP,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "integer",
        kind: TokenKind::Integer,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "intersect",
        kind: TokenKind::Intersect,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "interval",
        kind: TokenKind::Interval,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "into",
        kind: TokenKind::Into,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "invoker",
        kind: TokenKind::Invoker,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "is",
        kind: TokenKind::Is,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "isnull",
        kind: TokenKind::Isnull,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "isolation",
        kind: TokenKind::Isolation,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "join",
        kind: TokenKind::Join,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json",
        kind: TokenKind::Json,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_array",
        kind: TokenKind::JsonArray,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_arrayagg",
        kind: TokenKind::JsonArrayagg,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_exists",
        kind: TokenKind::JsonExists,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_object",
        kind: TokenKind::JsonObject,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_objectagg",
        kind: TokenKind::JsonObjectagg,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_query",
        kind: TokenKind::JsonQuery,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_scalar",
        kind: TokenKind::JsonScalar,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_serialize",
        kind: TokenKind::JsonSerialize,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_table",
        kind: TokenKind::JsonTable,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "json_value",
        kind: TokenKind::JsonValue,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "keep",
        kind: TokenKind::Keep,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "key",
        kind: TokenKind::Key,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "keys",
        kind: TokenKind::Keys,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "label",
        kind: TokenKind::Label,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "language",
        kind: TokenKind::Language,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "large",
        kind: TokenKind::LargeP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "last",
        kind: TokenKind::LastP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "lateral",
        kind: TokenKind::LateralP,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "leading",
        kind: TokenKind::Leading,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "leakproof",
        kind: TokenKind::Leakproof,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "least",
        kind: TokenKind::Least,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "left",
        kind: TokenKind::Left,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "level",
        kind: TokenKind::Level,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "like",
        kind: TokenKind::Like,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "limit",
        kind: TokenKind::Limit,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "listen",
        kind: TokenKind::Listen,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "load",
        kind: TokenKind::Load,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "local",
        kind: TokenKind::Local,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "localtime",
        kind: TokenKind::Localtime,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "localtimestamp",
        kind: TokenKind::Localtimestamp,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "location",
        kind: TokenKind::Location,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "lock",
        kind: TokenKind::LockP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "locked",
        kind: TokenKind::Locked,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "logged",
        kind: TokenKind::Logged,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "lsn",
        kind: TokenKind::LsnP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "mapping",
        kind: TokenKind::Mapping,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "match",
        kind: TokenKind::Match,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "matched",
        kind: TokenKind::Matched,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "materialized",
        kind: TokenKind::Materialized,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "maxvalue",
        kind: TokenKind::Maxvalue,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "merge",
        kind: TokenKind::Merge,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "merge_action",
        kind: TokenKind::MergeAction,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "method",
        kind: TokenKind::Method,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "minute",
        kind: TokenKind::MinuteP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "minvalue",
        kind: TokenKind::Minvalue,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "mode",
        kind: TokenKind::Mode,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "month",
        kind: TokenKind::MonthP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "move",
        kind: TokenKind::Move,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "name",
        kind: TokenKind::NameP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "names",
        kind: TokenKind::Names,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "national",
        kind: TokenKind::National,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "natural",
        kind: TokenKind::Natural,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nchar",
        kind: TokenKind::Nchar,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nested",
        kind: TokenKind::Nested,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "new",
        kind: TokenKind::New,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "next",
        kind: TokenKind::Next,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nfc",
        kind: TokenKind::Nfc,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nfd",
        kind: TokenKind::Nfd,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nfkc",
        kind: TokenKind::Nfkc,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nfkd",
        kind: TokenKind::Nfkd,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "no",
        kind: TokenKind::No,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "node",
        kind: TokenKind::Node,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "none",
        kind: TokenKind::None,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "normalize",
        kind: TokenKind::Normalize,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "normalized",
        kind: TokenKind::Normalized,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "not",
        kind: TokenKind::Not,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nothing",
        kind: TokenKind::Nothing,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "notify",
        kind: TokenKind::Notify,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "notnull",
        kind: TokenKind::Notnull,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "nowait",
        kind: TokenKind::Nowait,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "null",
        kind: TokenKind::NullP,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nullif",
        kind: TokenKind::Nullif,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "nulls",
        kind: TokenKind::NullsP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "numeric",
        kind: TokenKind::Numeric,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "object",
        kind: TokenKind::ObjectP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "objects",
        kind: TokenKind::ObjectsP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "of",
        kind: TokenKind::Of,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "off",
        kind: TokenKind::Off,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "offset",
        kind: TokenKind::Offset,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "oids",
        kind: TokenKind::Oids,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "old",
        kind: TokenKind::Old,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "omit",
        kind: TokenKind::Omit,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "on",
        kind: TokenKind::On,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "only",
        kind: TokenKind::Only,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "operator",
        kind: TokenKind::Operator,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "option",
        kind: TokenKind::Option,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "options",
        kind: TokenKind::Options,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "or",
        kind: TokenKind::Or,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "order",
        kind: TokenKind::Order,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "ordinality",
        kind: TokenKind::Ordinality,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "others",
        kind: TokenKind::Others,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "out",
        kind: TokenKind::OutP,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "outer",
        kind: TokenKind::OuterP,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "over",
        kind: TokenKind::Over,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "overlaps",
        kind: TokenKind::Overlaps,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "overlay",
        kind: TokenKind::Overlay,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "overriding",
        kind: TokenKind::Overriding,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "owned",
        kind: TokenKind::Owned,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "owner",
        kind: TokenKind::Owner,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "parallel",
        kind: TokenKind::Parallel,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "parameter",
        kind: TokenKind::Parameter,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "parser",
        kind: TokenKind::Parser,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "partial",
        kind: TokenKind::Partial,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "partition",
        kind: TokenKind::Partition,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "partitions",
        kind: TokenKind::Partitions,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "passing",
        kind: TokenKind::Passing,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "password",
        kind: TokenKind::Password,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "path",
        kind: TokenKind::Path,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "period",
        kind: TokenKind::Period,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "placing",
        kind: TokenKind::Placing,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "plan",
        kind: TokenKind::Plan,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "plans",
        kind: TokenKind::Plans,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "policy",
        kind: TokenKind::Policy,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "portion",
        kind: TokenKind::Portion,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "position",
        kind: TokenKind::Position,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "preceding",
        kind: TokenKind::Preceding,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "precision",
        kind: TokenKind::Precision,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "prepare",
        kind: TokenKind::Prepare,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "prepared",
        kind: TokenKind::Prepared,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "preserve",
        kind: TokenKind::Preserve,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "primary",
        kind: TokenKind::Primary,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "prior",
        kind: TokenKind::Prior,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "privileges",
        kind: TokenKind::Privileges,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "procedural",
        kind: TokenKind::Procedural,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "procedure",
        kind: TokenKind::Procedure,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "procedures",
        kind: TokenKind::Procedures,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "program",
        kind: TokenKind::Program,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "properties",
        kind: TokenKind::Properties,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "property",
        kind: TokenKind::Property,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "publication",
        kind: TokenKind::Publication,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "quote",
        kind: TokenKind::Quote,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "quotes",
        kind: TokenKind::Quotes,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "range",
        kind: TokenKind::Range,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "read",
        kind: TokenKind::Read,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "real",
        kind: TokenKind::Real,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "reassign",
        kind: TokenKind::Reassign,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "recursive",
        kind: TokenKind::Recursive,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "ref",
        kind: TokenKind::RefP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "references",
        kind: TokenKind::References,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "referencing",
        kind: TokenKind::Referencing,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "refresh",
        kind: TokenKind::Refresh,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "reindex",
        kind: TokenKind::Reindex,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "relationship",
        kind: TokenKind::Relationship,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "relative",
        kind: TokenKind::RelativeP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "release",
        kind: TokenKind::Release,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "rename",
        kind: TokenKind::Rename,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "repack",
        kind: TokenKind::Repack,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "repeatable",
        kind: TokenKind::Repeatable,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "replace",
        kind: TokenKind::Replace,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "replica",
        kind: TokenKind::Replica,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "reset",
        kind: TokenKind::Reset,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "respect",
        kind: TokenKind::RespectP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "restart",
        kind: TokenKind::Restart,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "restrict",
        kind: TokenKind::Restrict,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "return",
        kind: TokenKind::Return,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "returning",
        kind: TokenKind::Returning,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "returns",
        kind: TokenKind::Returns,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "revoke",
        kind: TokenKind::Revoke,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "right",
        kind: TokenKind::Right,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "role",
        kind: TokenKind::Role,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "rollback",
        kind: TokenKind::Rollback,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "rollup",
        kind: TokenKind::Rollup,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "routine",
        kind: TokenKind::Routine,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "routines",
        kind: TokenKind::Routines,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "row",
        kind: TokenKind::Row,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "rows",
        kind: TokenKind::Rows,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "rule",
        kind: TokenKind::Rule,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "savepoint",
        kind: TokenKind::Savepoint,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "scalar",
        kind: TokenKind::Scalar,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "schema",
        kind: TokenKind::Schema,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "schemas",
        kind: TokenKind::Schemas,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "scroll",
        kind: TokenKind::Scroll,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "search",
        kind: TokenKind::Search,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "second",
        kind: TokenKind::SecondP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "security",
        kind: TokenKind::Security,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "select",
        kind: TokenKind::Select,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "sequence",
        kind: TokenKind::Sequence,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "sequences",
        kind: TokenKind::Sequences,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "serializable",
        kind: TokenKind::Serializable,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "server",
        kind: TokenKind::Server,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "session",
        kind: TokenKind::Session,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "session_user",
        kind: TokenKind::SessionUser,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "set",
        kind: TokenKind::Set,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "setof",
        kind: TokenKind::Setof,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "sets",
        kind: TokenKind::Sets,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "share",
        kind: TokenKind::Share,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "show",
        kind: TokenKind::Show,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "similar",
        kind: TokenKind::Similar,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "simple",
        kind: TokenKind::Simple,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "skip",
        kind: TokenKind::Skip,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "smallint",
        kind: TokenKind::Smallint,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "snapshot",
        kind: TokenKind::Snapshot,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "some",
        kind: TokenKind::Some,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "source",
        kind: TokenKind::Source,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "split",
        kind: TokenKind::Split,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "sql",
        kind: TokenKind::SqlP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "stable",
        kind: TokenKind::Stable,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "standalone",
        kind: TokenKind::StandaloneP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "start",
        kind: TokenKind::Start,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "statement",
        kind: TokenKind::Statement,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "statistics",
        kind: TokenKind::Statistics,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "stdin",
        kind: TokenKind::Stdin,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "stdout",
        kind: TokenKind::Stdout,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "storage",
        kind: TokenKind::Storage,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "stored",
        kind: TokenKind::Stored,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "strict",
        kind: TokenKind::StrictP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "string",
        kind: TokenKind::StringP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "strip",
        kind: TokenKind::StripP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "subscription",
        kind: TokenKind::Subscription,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "substring",
        kind: TokenKind::Substring,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "support",
        kind: TokenKind::Support,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "symmetric",
        kind: TokenKind::Symmetric,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "sysid",
        kind: TokenKind::Sysid,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "system",
        kind: TokenKind::SystemP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "system_user",
        kind: TokenKind::SystemUser,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "table",
        kind: TokenKind::Table,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "tables",
        kind: TokenKind::Tables,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "tablesample",
        kind: TokenKind::Tablesample,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "tablespace",
        kind: TokenKind::Tablespace,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "target",
        kind: TokenKind::Target,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "temp",
        kind: TokenKind::Temp,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "template",
        kind: TokenKind::Template,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "temporary",
        kind: TokenKind::Temporary,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "text",
        kind: TokenKind::TextP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "then",
        kind: TokenKind::Then,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "ties",
        kind: TokenKind::Ties,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "time",
        kind: TokenKind::Time,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "timestamp",
        kind: TokenKind::Timestamp,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "to",
        kind: TokenKind::To,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "trailing",
        kind: TokenKind::Trailing,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "transaction",
        kind: TokenKind::Transaction,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "transform",
        kind: TokenKind::Transform,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "treat",
        kind: TokenKind::Treat,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "trigger",
        kind: TokenKind::Trigger,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "trim",
        kind: TokenKind::Trim,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "true",
        kind: TokenKind::TrueP,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "truncate",
        kind: TokenKind::Truncate,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "trusted",
        kind: TokenKind::Trusted,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "type",
        kind: TokenKind::TypeP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "types",
        kind: TokenKind::TypesP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "uescape",
        kind: TokenKind::Uescape,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "unbounded",
        kind: TokenKind::Unbounded,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "uncommitted",
        kind: TokenKind::Uncommitted,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "unconditional",
        kind: TokenKind::Unconditional,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "unencrypted",
        kind: TokenKind::Unencrypted,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "union",
        kind: TokenKind::Union,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "unique",
        kind: TokenKind::Unique,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "unknown",
        kind: TokenKind::Unknown,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "unlisten",
        kind: TokenKind::Unlisten,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "unlogged",
        kind: TokenKind::Unlogged,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "until",
        kind: TokenKind::Until,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "update",
        kind: TokenKind::Update,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "user",
        kind: TokenKind::User,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "using",
        kind: TokenKind::Using,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "vacuum",
        kind: TokenKind::Vacuum,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "valid",
        kind: TokenKind::Valid,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "validate",
        kind: TokenKind::Validate,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "validator",
        kind: TokenKind::Validator,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "value",
        kind: TokenKind::ValueP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "values",
        kind: TokenKind::Values,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "varchar",
        kind: TokenKind::Varchar,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "variadic",
        kind: TokenKind::Variadic,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "varying",
        kind: TokenKind::Varying,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "verbose",
        kind: TokenKind::Verbose,
        category: KeywordCategory::TypeFuncName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "version",
        kind: TokenKind::VersionP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "vertex",
        kind: TokenKind::Vertex,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "view",
        kind: TokenKind::View,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "views",
        kind: TokenKind::Views,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "virtual",
        kind: TokenKind::Virtual,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "volatile",
        kind: TokenKind::Volatile,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "wait",
        kind: TokenKind::Wait,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "when",
        kind: TokenKind::When,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "where",
        kind: TokenKind::Where,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "whitespace",
        kind: TokenKind::WhitespaceP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "window",
        kind: TokenKind::Window,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "with",
        kind: TokenKind::With,
        category: KeywordCategory::Reserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "within",
        kind: TokenKind::Within,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "without",
        kind: TokenKind::Without,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "work",
        kind: TokenKind::Work,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "wrapper",
        kind: TokenKind::Wrapper,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "write",
        kind: TokenKind::Write,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xml",
        kind: TokenKind::XmlP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlattributes",
        kind: TokenKind::Xmlattributes,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlconcat",
        kind: TokenKind::Xmlconcat,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlelement",
        kind: TokenKind::Xmlelement,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlexists",
        kind: TokenKind::Xmlexists,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlforest",
        kind: TokenKind::Xmlforest,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlnamespaces",
        kind: TokenKind::Xmlnamespaces,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlparse",
        kind: TokenKind::Xmlparse,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlpi",
        kind: TokenKind::Xmlpi,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlroot",
        kind: TokenKind::Xmlroot,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmlserialize",
        kind: TokenKind::Xmlserialize,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "xmltable",
        kind: TokenKind::Xmltable,
        category: KeywordCategory::ColName,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "year",
        kind: TokenKind::YearP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::As,
    },
    Keyword {
        word: "yes",
        kind: TokenKind::YesP,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
    Keyword {
        word: "zone",
        kind: TokenKind::Zone,
        category: KeywordCategory::Unreserved,
        bare_label: BareLabel::Bare,
    },
];

pub fn lookup_keyword(word: &str) -> Option<&'static Keyword> {
    let lower = word.to_ascii_lowercase();
    KEYWORDS
        .binary_search_by(|keyword| keyword.word.cmp(lower.as_str()))
        .ok()
        .map(|index| &KEYWORDS[index])
}

pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            return Ok(tokens);
        }
    }
}

pub struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments()?;
        let location = self.pos;
        if self.eof() {
            return Ok(Token::new(TokenKind::Eof, location));
        }

        if self.starts_with_ignore_ascii_case("b'") {
            self.pos += 2;
            return self.scan_bit_or_hex_string(location, b'b', TokenKind::BConst);
        }
        if self.starts_with_ignore_ascii_case("x'") {
            self.pos += 2;
            return self.scan_bit_or_hex_string(location, b'x', TokenKind::XConst);
        }
        if self.starts_with_ignore_ascii_case("n'") {
            self.pos += 1;
            return Ok(Token::keyword(TokenKind::Nchar, location, "nchar"));
        }
        if self.starts_with_ignore_ascii_case("e'") {
            self.pos += 2;
            return self.scan_quoted_string(location, StringMode::Extended, TokenKind::SConst);
        }
        if self.starts_with_ignore_ascii_case("u&\"") {
            self.pos += 3;
            return self.scan_quoted_identifier(location, true);
        }
        if self.starts_with_ignore_ascii_case("u&'") {
            self.pos += 3;
            return self.scan_quoted_string(location, StringMode::Unicode, TokenKind::USConst);
        }
        if self.starts_with_ignore_ascii_case("u&") {
            self.pos += 1;
            return Ok(Token::string(TokenKind::Ident, location, "u"));
        }
        if self.peek() == Some(b'\'') {
            self.pos += 1;
            return self.scan_quoted_string(location, StringMode::Standard, TokenKind::SConst);
        }
        if self.peek() == Some(b'"') {
            self.pos += 1;
            return self.scan_quoted_identifier(location, false);
        }
        if self.peek() == Some(b'$') {
            if let Some(token) = self.try_scan_dollar_or_param(location)? {
                return Ok(token);
            }
        }

        if self.starts_with("::") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::TypeCast, location));
        }
        if self.starts_with("..") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::DotDot, location));
        }
        if self.starts_with(":=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::ColonEquals, location));
        }
        if self.starts_with("=>") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::EqualsGreater, location));
        }
        if self.starts_with("<=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::LessEquals, location));
        }
        if self.starts_with(">=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::GreaterEquals, location));
        }
        if self.starts_with("<>") || self.starts_with("!=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::NotEquals, location));
        }
        if self.starts_with("->") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::RightArrow, location));
        }

        if self.peek().is_some_and(is_dec_digit)
            || (self.peek() == Some(b'.') && self.peek_n(1).is_some_and(is_dec_digit))
        {
            return self.scan_number(location);
        }

        if self.peek().is_some_and(is_ident_start) {
            return Ok(self.scan_identifier_or_keyword(location));
        }

        if self.peek().is_some_and(is_operator_char) {
            return self.scan_operator(location);
        }

        if self.peek().is_some_and(is_self_char) {
            let ch = self.bump_ascii_char().unwrap();
            return Ok(Token::new(TokenKind::Char(ch), location));
        }

        let ch = self.bump_char().unwrap_or('\0');
        Ok(Token::new(TokenKind::Char(ch), location))
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_n(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.pos + n).copied()
    }

    fn starts_with(&self, needle: &str) -> bool {
        self.bytes[self.pos..].starts_with(needle.as_bytes())
    }

    fn starts_with_ignore_ascii_case(&self, needle: &str) -> bool {
        let hay = self.bytes.get(self.pos..self.pos + needle.len());
        hay.is_some_and(|hay| hay.eq_ignore_ascii_case(needle.as_bytes()))
    }

    fn bump_ascii_char(&mut self) -> Option<char> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b as char)
    }

    fn bump_char(&mut self) -> Option<char> {
        let rest = self.input.get(self.pos..)?;
        let ch = rest.chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn error<T>(
        &self,
        location: usize,
        message: impl Into<std::string::String>,
    ) -> Result<T, LexError> {
        Err(LexError::new(location, message))
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            let start = self.pos;
            while self.peek().is_some_and(is_space) {
                self.pos += 1;
            }
            if self.starts_with("--") {
                self.pos += 2;
                while let Some(b) = self.peek() {
                    if b == b'\n' || b == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
                continue;
            }
            if self.starts_with("/*") {
                self.skip_block_comment()?;
                continue;
            }
            if self.pos == start {
                return Ok(());
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let location = self.pos;
        self.pos += 2;
        let mut depth = 0usize;
        while !self.eof() {
            if self.starts_with("/*") {
                depth += 1;
                self.pos += 2;
            } else if self.starts_with("*/") {
                self.pos += 2;
                if depth == 0 {
                    return Ok(());
                }
                depth -= 1;
            } else {
                self.pos += 1;
            }
        }
        self.error(location, "unterminated /* comment")
    }

    fn scan_bit_or_hex_string(
        &mut self,
        location: usize,
        prefix: u8,
        kind: TokenKind,
    ) -> Result<Token, LexError> {
        let mut literal = vec![prefix];
        loop {
            while let Some(b) = self.peek() {
                if b == b'\'' {
                    break;
                }
                literal.push(b);
                self.pos += 1;
            }
            if self.eof() {
                let msg = if prefix == b'b' {
                    "unterminated bit string literal"
                } else {
                    "unterminated hexadecimal string literal"
                };
                return self.error(location, msg);
            }
            self.pos += 1;
            if self.consume_quote_continuation() {
                continue;
            }
            return Ok(Token::string(kind, location, string_from_bytes(literal)));
        }
    }

    fn scan_quoted_string(
        &mut self,
        location: usize,
        mode: StringMode,
        kind: TokenKind,
    ) -> Result<Token, LexError> {
        let mut literal = Vec::new();
        loop {
            while let Some(b) = self.peek() {
                if b == b'\'' {
                    if self.peek_n(1) == Some(b'\'') {
                        literal.push(b'\'');
                        self.pos += 2;
                        continue;
                    }
                    self.pos += 1;
                    if self.consume_quote_continuation() {
                        continue;
                    }
                    return Ok(Token::string(kind, location, string_from_bytes(literal)));
                }

                if mode == StringMode::Extended && b == b'\\' {
                    self.pos += 1;
                    self.scan_escape_sequence(location, &mut literal)?;
                    continue;
                }

                literal.push(b);
                self.pos += 1;
            }
            return self.error(location, "unterminated quoted string");
        }
    }

    fn scan_escape_sequence(
        &mut self,
        location: usize,
        literal: &mut Vec<u8>,
    ) -> Result<(), LexError> {
        let Some(next) = self.peek() else {
            literal.push(b'\\');
            return Ok(());
        };

        if next == b'u' || next == b'U' {
            let escape_location = self.pos - 1;
            let width = if next == b'u' { 4 } else { 8 };
            self.pos += 1;
            let first = self.read_fixed_hex_escape(escape_location, width)?;
            if is_utf16_surrogate_first(first) {
                if !(self.peek() == Some(b'\\') && matches!(self.peek_n(1), Some(b'u' | b'U'))) {
                    return self.error(self.pos, "invalid Unicode surrogate pair");
                }
                self.pos += 1;
                let second_width = if self.peek() == Some(b'u') { 4 } else { 8 };
                self.pos += 1;
                let second = self.read_fixed_hex_escape(self.pos - 2, second_width)?;
                if !is_utf16_surrogate_second(second) {
                    return self.error(self.pos, "invalid Unicode surrogate pair");
                }
                let codepoint = 0x10000 + (((first - 0xD800) << 10) | (second - 0xDC00));
                push_codepoint(literal, codepoint, escape_location)?;
            } else if is_utf16_surrogate_second(first) {
                return self.error(escape_location, "invalid Unicode surrogate pair");
            } else {
                push_codepoint(literal, first, escape_location)?;
            }
            return Ok(());
        }

        if (b'0'..=b'7').contains(&next) {
            let start = self.pos;
            let mut end = self.pos;
            for _ in 0..3 {
                if self
                    .bytes
                    .get(end)
                    .is_some_and(|b| (b'0'..=b'7').contains(b))
                {
                    end += 1;
                } else {
                    break;
                }
            }
            let value = u8::from_str_radix(&self.input[start..end], 8).unwrap();
            literal.push(value);
            self.pos = end;
            return Ok(());
        }

        if next == b'x' {
            let start = self.pos + 1;
            let mut end = start;
            for _ in 0..2 {
                if self.bytes.get(end).is_some_and(|b| b.is_ascii_hexdigit()) {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                let value = u8::from_str_radix(&self.input[start..end], 16).unwrap();
                literal.push(value);
                self.pos = end;
                return Ok(());
            }
        }

        self.pos += 1;
        literal.push(match next {
            b'b' => 0x08,
            b'f' => 0x0C,
            b'n' => b'\n',
            b'r' => b'\n',
            b't' => b'\t',
            b'v' => 0x0B,
            other => other,
        });
        let _ = location;
        Ok(())
    }

    fn read_fixed_hex_escape(&mut self, location: usize, width: usize) -> Result<u32, LexError> {
        let end = self.pos + width;
        if end > self.bytes.len() || !self.bytes[self.pos..end].iter().all(u8::is_ascii_hexdigit) {
            return self.error(location, "invalid Unicode escape");
        }
        let value = u32::from_str_radix(&self.input[self.pos..end], 16).unwrap();
        self.pos = end;
        Ok(value)
    }

    fn consume_quote_continuation(&mut self) -> bool {
        let after_quote = self.pos;
        let mut pos = self.pos;
        let mut saw_newline = false;
        loop {
            match self.bytes.get(pos).copied() {
                Some(b'\n' | b'\r') => {
                    saw_newline = true;
                    pos += 1;
                }
                Some(b' ' | b'\t' | 0x0C | 0x0B) => pos += 1,
                Some(b'-') if self.bytes.get(pos + 1) == Some(&b'-') => {
                    pos += 2;
                    while let Some(b) = self.bytes.get(pos).copied() {
                        if b == b'\n' || b == b'\r' {
                            break;
                        }
                        pos += 1;
                    }
                }
                _ => break,
            }
        }
        if saw_newline && self.bytes.get(pos) == Some(&b'\'') {
            self.pos = pos + 1;
            true
        } else {
            self.pos = after_quote;
            false
        }
    }

    fn try_scan_dollar_or_param(&mut self, location: usize) -> Result<Option<Token>, LexError> {
        if let Some(delim_end) = self.dollar_delimiter_end(self.pos) {
            let delimiter = &self.input[self.pos..delim_end];
            self.pos = delim_end;
            let content_start = self.pos;
            if let Some(relative_end) = self.input[self.pos..].find(delimiter) {
                let content_end = content_start + relative_end;
                let value = self.input[content_start..content_end].to_owned();
                self.pos = content_end + delimiter.len();
                return Ok(Some(Token::string(TokenKind::SConst, location, value)));
            }
            return self.error(location, "unterminated dollar-quoted string");
        }

        if self.peek_n(1).is_some_and(is_ident_start) {
            self.pos += 1;
            return Ok(Some(Token::new(TokenKind::Char('$'), location)));
        }

        if self.peek_n(1).is_some_and(is_dec_digit) {
            self.pos += 1;
            let start = self.pos;
            while self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
            }
            if self.peek().is_some_and(is_ident_start) {
                return self.error(location, "trailing junk after parameter");
            }
            let raw = &self.input[start..self.pos];
            let value = raw
                .parse::<i32>()
                .map_err(|_| LexError::new(location, "parameter number too large"))?;
            return Ok(Some(Token::integer(TokenKind::Param, location, value)));
        }

        Ok(None)
    }

    fn dollar_delimiter_end(&self, start: usize) -> Option<usize> {
        if self.bytes.get(start) != Some(&b'$') {
            return None;
        }
        let mut pos = start + 1;
        if self.bytes.get(pos) == Some(&b'$') {
            return Some(pos + 1);
        }
        if !self.bytes.get(pos).is_some_and(|b| is_dolq_start(*b)) {
            return None;
        }
        pos += 1;
        while self.bytes.get(pos).is_some_and(|b| is_dolq_cont(*b)) {
            pos += 1;
        }
        if self.bytes.get(pos) == Some(&b'$') {
            Some(pos + 1)
        } else {
            None
        }
    }

    fn scan_quoted_identifier(
        &mut self,
        location: usize,
        unicode: bool,
    ) -> Result<Token, LexError> {
        let mut literal = Vec::new();
        while let Some(b) = self.peek() {
            if b == b'"' {
                if self.peek_n(1) == Some(b'"') {
                    literal.push(b'"');
                    self.pos += 2;
                    continue;
                }
                self.pos += 1;
                if literal.is_empty() {
                    return self.error(location, "zero-length delimited identifier");
                }
                let ident = truncate_identifier(&string_from_bytes(literal));
                let kind = if unicode {
                    TokenKind::UIdent
                } else {
                    TokenKind::Ident
                };
                return Ok(Token::string(kind, location, ident));
            }
            literal.push(b);
            self.pos += 1;
        }
        self.error(location, "unterminated quoted identifier")
    }

    fn scan_identifier_or_keyword(&mut self, location: usize) -> Token {
        let start = self.pos;
        self.pos += 1;
        while self.peek().is_some_and(is_ident_cont) {
            self.pos += 1;
        }
        let raw = &self.input[start..self.pos];
        if let Some(keyword) = lookup_keyword(raw) {
            return Token::keyword(keyword.kind, location, keyword.word);
        }
        Token::string(
            TokenKind::Ident,
            location,
            downcase_truncate_identifier(raw),
        )
    }

    fn scan_number(&mut self, location: usize) -> Result<Token, LexError> {
        let start = self.pos;
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.scan_decinteger_tail();
            self.scan_exponent(location)?;
            self.reject_numeric_junk(location)?;
            return Ok(Token::string(
                TokenKind::FConst,
                location,
                &self.input[start..self.pos],
            ));
        }

        if self.starts_with_ignore_ascii_case("0x") {
            return self.scan_prefixed_integer(location, 16, "invalid hexadecimal integer");
        }
        if self.starts_with_ignore_ascii_case("0o") {
            return self.scan_prefixed_integer(location, 8, "invalid octal integer");
        }
        if self.starts_with_ignore_ascii_case("0b") {
            return self.scan_prefixed_integer(location, 2, "invalid binary integer");
        }

        self.pos += 1;
        self.scan_decinteger_tail();

        if self.starts_with("..") {
            return self.integer_or_float(location, start, 10, "");
        }

        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
                self.scan_decinteger_tail();
            }
        }
        if self.peek().is_some_and(|b| b == b'e' || b == b'E') {
            is_float = true;
            self.scan_exponent(location)?;
        }
        self.reject_numeric_junk(location)?;
        if is_float {
            Ok(Token::string(
                TokenKind::FConst,
                location,
                &self.input[start..self.pos],
            ))
        } else {
            self.integer_or_float(location, start, 10, "")
        }
    }

    fn scan_prefixed_integer(
        &mut self,
        location: usize,
        radix: u32,
        fail_message: &'static str,
    ) -> Result<Token, LexError> {
        let start = self.pos;
        self.pos += 2;
        let digit_start = self.pos;
        let mut saw_digit = false;
        if self.peek() == Some(b'_') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|b| is_digit_for_radix(b, radix)) {
            saw_digit = true;
            self.pos += 1;
            if self.peek() == Some(b'_')
                && self.peek_n(1).is_some_and(|b| is_digit_for_radix(b, radix))
            {
                self.pos += 1;
            }
        }
        if !saw_digit || self.pos == digit_start {
            return self.error(location, fail_message);
        }
        self.reject_numeric_junk(location)?;
        self.integer_or_float(location, start, radix, prefix_for_radix(radix))
    }

    fn scan_decinteger_tail(&mut self) {
        loop {
            if self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
            } else if self.peek() == Some(b'_') && self.peek_n(1).is_some_and(is_dec_digit) {
                self.pos += 2;
            } else {
                break;
            }
        }
    }

    fn scan_exponent(&mut self, location: usize) -> Result<(), LexError> {
        if !self.peek().is_some_and(|b| b == b'e' || b == b'E') {
            return Ok(());
        }
        let save = self.pos;
        self.pos += 1;
        if self.peek().is_some_and(|b| b == b'+' || b == b'-') {
            self.pos += 1;
        }
        if !self.peek().is_some_and(is_dec_digit) {
            self.pos = save;
            return self.error(location, "trailing junk after numeric literal");
        }
        self.pos += 1;
        self.scan_decinteger_tail();
        Ok(())
    }

    fn reject_numeric_junk(&self, location: usize) -> Result<(), LexError> {
        if self.peek().is_some_and(is_ident_start) {
            return Err(LexError::new(
                location,
                "trailing junk after numeric literal",
            ));
        }
        Ok(())
    }

    fn integer_or_float(
        &self,
        location: usize,
        start: usize,
        radix: u32,
        prefix: &str,
    ) -> Result<Token, LexError> {
        let raw = &self.input[start..self.pos];
        let cleaned = raw.replace('_', "");
        let digits = if radix == 10 {
            cleaned.as_str()
        } else {
            cleaned.get(prefix.len()..).unwrap_or(cleaned.as_str())
        };
        match i32::from_str_radix(digits, radix) {
            Ok(value) => Ok(Token::integer(TokenKind::IConst, location, value)),
            Err(_) => Ok(Token::string(TokenKind::FConst, location, raw)),
        }
    }

    fn scan_operator(&mut self, location: usize) -> Result<Token, LexError> {
        let start = self.pos;
        while self.peek().is_some_and(is_operator_char) {
            if self.starts_with("/*") || self.starts_with("--") {
                break;
            }
            self.pos += 1;
        }
        let mut end = self.pos;

        if end - start > 1 {
            let bytes = &self.bytes[start..end];
            if matches!(bytes.last(), Some(b'+' | b'-')) {
                let has_non_sql = bytes[..bytes.len() - 1].iter().any(|b| {
                    matches!(
                        b,
                        b'~' | b'!' | b'@' | b'#' | b'^' | b'&' | b'|' | b'`' | b'?' | b'%'
                    )
                });
                if !has_non_sql {
                    while end - start > 1 && matches!(self.bytes[end - 1], b'+' | b'-') {
                        end -= 1;
                    }
                    self.pos = end;
                }
            }
        }

        let op = &self.input[start..end];
        if op.len() == 1 {
            let b = op.as_bytes()[0];
            if is_self_char(b) {
                return Ok(Token::new(TokenKind::Char(b as char), location));
            }
        }
        if op.len() == 2 {
            let kind = match op {
                "=>" => Some(TokenKind::EqualsGreater),
                ">=" => Some(TokenKind::GreaterEquals),
                "<=" => Some(TokenKind::LessEquals),
                "<>" | "!=" => Some(TokenKind::NotEquals),
                "->" => Some(TokenKind::RightArrow),
                _ => None,
            };
            if let Some(kind) = kind {
                return Ok(Token::new(kind, location));
            }
        }
        if op.len() >= NAMEDATALEN {
            return self.error(location, "operator too long");
        }
        Ok(Token::string(TokenKind::Op, location, op))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringMode {
    Standard,
    Extended,
    Unicode,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x0B)
}

fn is_dec_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

fn is_ident_cont(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit() || b == b'$'
}

fn is_dolq_start(b: u8) -> bool {
    is_ident_start(b)
}

fn is_dolq_cont(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn is_self_char(b: u8) -> bool {
    matches!(
        b,
        b',' | b'('
            | b')'
            | b'['
            | b']'
            | b'.'
            | b';'
            | b':'
            | b'|'
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'^'
            | b'<'
            | b'>'
            | b'='
    )
}

fn is_operator_char(b: u8) -> bool {
    matches!(
        b,
        b'~' | b'!'
            | b'@'
            | b'#'
            | b'^'
            | b'&'
            | b'|'
            | b'`'
            | b'?'
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'<'
            | b'>'
            | b'='
    )
}

fn is_digit_for_radix(b: u8, radix: u32) -> bool {
    match radix {
        2 => matches!(b, b'0' | b'1'),
        8 => matches!(b, b'0'..=b'7'),
        10 => b.is_ascii_digit(),
        16 => b.is_ascii_hexdigit(),
        _ => false,
    }
}

fn prefix_for_radix(radix: u32) -> &'static str {
    match radix {
        2 => "0b",
        8 => "0o",
        16 => "0x",
        _ => "",
    }
}

fn downcase_truncate_identifier(raw: &str) -> std::string::String {
    truncate_identifier(&raw.to_ascii_lowercase())
}

fn truncate_identifier(raw: &str) -> std::string::String {
    let max = NAMEDATALEN - 1;
    if raw.len() <= max {
        return raw.to_owned();
    }
    let mut end = max;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    raw[..end].to_owned()
}

fn string_from_bytes(bytes: Vec<u8>) -> std::string::String {
    match std::string::String::from_utf8(bytes) {
        Ok(value) => value,
        Err(err) => std::string::String::from_utf8_lossy(err.as_bytes()).into_owned(),
    }
}

fn is_utf16_surrogate_first(c: u32) -> bool {
    (0xD800..=0xDBFF).contains(&c)
}

fn is_utf16_surrogate_second(c: u32) -> bool {
    (0xDC00..=0xDFFF).contains(&c)
}

fn push_codepoint(literal: &mut Vec<u8>, codepoint: u32, location: usize) -> Result<(), LexError> {
    let Some(ch) = char::from_u32(codepoint) else {
        return Err(LexError::new(location, "invalid Unicode escape value"));
    };
    let mut buf = [0; 4];
    literal.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(sql: &str) -> Vec<TokenKind> {
        lex(sql)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_keywords_identifiers_and_punctuation() {
        assert_eq!(
            kinds("SELECT foo, $1 FROM bar::int"),
            vec![
                TokenKind::Select,
                TokenKind::Ident,
                TokenKind::Char(','),
                TokenKind::Param,
                TokenKind::From,
                TokenKind::Ident,
                TokenKind::TypeCast,
                TokenKind::IntP,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_standard_extended_and_dollar_strings() {
        let tokens = lex("'a''b' E'c\\n' $$raw$$").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("a'b".into())));
        assert_eq!(tokens[1].value, Some(TokenValue::String("c\n".into())));
        assert_eq!(tokens[2].value, Some(TokenValue::String("raw".into())));
    }

    #[test]
    fn concatenates_adjacent_strings_only_across_newline() {
        let tokens = lex("'a'
'b'")
        .unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("ab".into())));
        assert_eq!(tokens[1].kind, TokenKind::Eof);
    }

    #[test]
    fn lexes_prefixed_numbers_and_numeric_fail() {
        let tokens = lex("0x10 0X11 0o10 0b10 1..10 1.5e2").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::Integer(16)));
        assert_eq!(tokens[1].value, Some(TokenValue::Integer(17)));
        assert_eq!(tokens[2].value, Some(TokenValue::Integer(8)));
        assert_eq!(tokens[3].value, Some(TokenValue::Integer(2)));
        assert_eq!(tokens[4].value, Some(TokenValue::Integer(1)));
        assert_eq!(tokens[5].kind, TokenKind::DotDot);
        assert_eq!(tokens[6].value, Some(TokenValue::Integer(10)));
        assert_eq!(tokens[7].kind, TokenKind::FConst);
    }

    #[test]
    fn handles_nested_comments_and_operator_comment_boundaries() {
        assert_eq!(
            kinds("1 /* outer /* inner */ done */ +/*comment*/ 2"),
            vec![
                TokenKind::IConst,
                TokenKind::Char('+'),
                TokenKind::IConst,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn rejects_trailing_numeric_junk() {
        assert!(lex("123abc").is_err());
        assert!(lex("$1abc").is_err());
    }
}
