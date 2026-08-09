//! PostgreSQL raw parse-tree types expressed with idiomatic Rust names.
//!
//! The source of truth is PostgreSQL's `src/include/nodes/*.h`, not the grammar
//! output. Parser modules construct these types directly, so their field defaults
//! and [`Node`] variants are part of the parser's compatibility contract.

mod enums;
mod helpers;
mod keywords;

pub use enums::*;
pub(crate) use helpers::*;
pub use keywords::*;

pub type ParseLoc = i32;
pub type Oid = u32;
pub type Index = u32;
pub type AttrNumber = i16;
pub type AclMode = u64;
pub type Datum = u64;
pub type RelFileNumber = u32;
pub type Relids = Bitmapset;
pub type SubTransactionId = u32;
pub type Selectivity = f64;
pub type Cost = f64;
pub type Cardinality = f64;
pub type Bits8 = u8;
pub type Size = usize;
pub type StrategyNumber = u16;
pub type SubLinkId = u32;
pub type Bitmapset = Vec<i32>;
pub type NodeList = Vec<Node>;

pub const ACL_INSERT: AclMode = 1 << 0;
pub const ACL_SELECT: AclMode = 1 << 1;
pub const ACL_UPDATE: AclMode = 1 << 2;
pub const ACL_DELETE: AclMode = 1 << 3;
pub const ACL_TRUNCATE: AclMode = 1 << 4;
pub const ACL_REFERENCES: AclMode = 1 << 5;
pub const ACL_TRIGGER: AclMode = 1 << 6;
pub const ACL_EXECUTE: AclMode = 1 << 7;
pub const ACL_USAGE: AclMode = 1 << 8;
pub const ACL_CREATE: AclMode = 1 << 9;
pub const ACL_CREATE_TEMP: AclMode = 1 << 10;
pub const ACL_CONNECT: AclMode = 1 << 11;
pub const ACL_SET: AclMode = 1 << 12;
pub const ACL_ALTER_SYSTEM: AclMode = 1 << 13;
pub const ACL_MAINTAIN: AclMode = 1 << 14;
pub const ACL_NO_RIGHTS: AclMode = 0;
pub const ACL_SELECT_FOR_UPDATE: AclMode = ACL_UPDATE;

pub const FRAMEOPTION_NONDEFAULT: i32 = 0x00001;
pub const FRAMEOPTION_RANGE: i32 = 0x00002;
pub const FRAMEOPTION_ROWS: i32 = 0x00004;
pub const FRAMEOPTION_GROUPS: i32 = 0x00008;
pub const FRAMEOPTION_BETWEEN: i32 = 0x00010;
pub const FRAMEOPTION_START_UNBOUNDED_PRECEDING: i32 = 0x00020;
pub const FRAMEOPTION_END_UNBOUNDED_PRECEDING: i32 = 0x00040;
pub const FRAMEOPTION_START_UNBOUNDED_FOLLOWING: i32 = 0x00080;
pub const FRAMEOPTION_END_UNBOUNDED_FOLLOWING: i32 = 0x00100;
pub const FRAMEOPTION_START_CURRENT_ROW: i32 = 0x00200;
pub const FRAMEOPTION_END_CURRENT_ROW: i32 = 0x00400;
pub const FRAMEOPTION_START_OFFSET_PRECEDING: i32 = 0x00800;
pub const FRAMEOPTION_END_OFFSET_PRECEDING: i32 = 0x01000;
pub const FRAMEOPTION_START_OFFSET_FOLLOWING: i32 = 0x02000;
pub const FRAMEOPTION_END_OFFSET_FOLLOWING: i32 = 0x04000;
pub const FRAMEOPTION_EXCLUDE_CURRENT_ROW: i32 = 0x08000;
pub const FRAMEOPTION_EXCLUDE_GROUP: i32 = 0x10000;
pub const FRAMEOPTION_EXCLUDE_TIES: i32 = 0x20000;
pub const FRAMEOPTION_START_OFFSET: i32 =
    FRAMEOPTION_START_OFFSET_PRECEDING | FRAMEOPTION_START_OFFSET_FOLLOWING;
pub const FRAMEOPTION_END_OFFSET: i32 =
    FRAMEOPTION_END_OFFSET_PRECEDING | FRAMEOPTION_END_OFFSET_FOLLOWING;
pub const FRAMEOPTION_EXCLUSION: i32 =
    FRAMEOPTION_EXCLUDE_CURRENT_ROW | FRAMEOPTION_EXCLUDE_GROUP | FRAMEOPTION_EXCLUDE_TIES;
pub const FRAMEOPTION_DEFAULTS: i32 =
    FRAMEOPTION_RANGE | FRAMEOPTION_START_UNBOUNDED_PRECEDING | FRAMEOPTION_END_CURRENT_ROW;

pub const FKCONSTR_ACTION_NOACTION: u8 = b'a';
pub const FKCONSTR_ACTION_RESTRICT: u8 = b'r';
pub const FKCONSTR_ACTION_CASCADE: u8 = b'c';
pub const FKCONSTR_ACTION_SETNULL: u8 = b'n';
pub const FKCONSTR_ACTION_SETDEFAULT: u8 = b'd';
pub const FKCONSTR_MATCH_FULL: u8 = b'f';
pub const FKCONSTR_MATCH_PARTIAL: u8 = b'p';
pub const FKCONSTR_MATCH_SIMPLE: u8 = b's';

pub const CURSOR_OPT_BINARY: i32 = 0x0001;
pub const CURSOR_OPT_SCROLL: i32 = 0x0002;
pub const CURSOR_OPT_NO_SCROLL: i32 = 0x0004;
pub const CURSOR_OPT_INSENSITIVE: i32 = 0x0008;
pub const CURSOR_OPT_ASENSITIVE: i32 = 0x0010;
pub const CURSOR_OPT_HOLD: i32 = 0x0020;
pub const CURSOR_OPT_FAST_PLAN: i32 = 0x0100;
pub const CURSOR_OPT_GENERIC_PLAN: i32 = 0x0200;
pub const CURSOR_OPT_CUSTOM_PLAN: i32 = 0x0400;
pub const CURSOR_OPT_PARALLEL_OK: i32 = 0x0800;

pub const AGGSPLITOP_COMBINE: i32 = 0x01;
pub const AGGSPLITOP_SKIPFINAL: i32 = 0x02;
pub const AGGSPLITOP_SERIALIZE: i32 = 0x04;
pub const AGGSPLITOP_DESERIALIZE: i32 = 0x08;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum CmdType {
    #[default]
    Unknown,
    Select,
    Update,
    Insert,
    Delete,
    Merge,
    Utility,
    Nothing,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JoinType {
    #[default]
    Inner,
    Left,
    Full,
    Right,
    Semi,
    Anti,
    RightSemi,
    RightAnti,
    UniqueOuter,
    UniqueInner,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AggStrategy {
    #[default]
    Plain,
    Sorted,
    Hashed,
    Mixed,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AggSplit {
    #[default]
    Simple = 0,
    InitialSerial = 0x02 | 0x04,
    FinalDeserial = 0x01 | 0x08,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SetOpCmd {
    #[default]
    Intersect,
    IntersectAll,
    Except,
    ExceptAll,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SetOpStrategy {
    #[default]
    Sorted,
    Hashed,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum OnConflictAction {
    #[default]
    None,
    Nothing,
    Update,
    Select,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum LimitOption {
    #[default]
    Count,
    WithTies,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum LockClauseStrength {
    #[default]
    None,
    Forkeyshare,
    Forshare,
    Fornokeyupdate,
    Forupdate,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum LockWaitPolicy {
    #[default]
    Block,
    Skip,
    Error,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum LockTupleMode {
    #[default]
    KeyShare,
    Share,
    NoKeyExclusive,
    Exclusive,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum CompareType {
    #[default]
    Invalid = 0,
    Lt = 1,
    Le = 2,
    Eq = 3,
    Ge = 4,
    Gt = 5,
    Ne = 6,
    Overlap,
    ContainedBy,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ScanDirection {
    #[default]
    BackwardScanDirection = -1,
    NoMovementScanDirection = 0,
    ForwardScanDirection = 1,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum OverridingKind {
    #[default]
    NotSet = 0,
    UserValue,
    SystemValue,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum OnCommitAction {
    #[default]
    Noop,
    PreserveRows,
    DeleteRows,
    Drop,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum TableFuncType {
    #[default]
    Xmltable,
    JsonTable,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum VarReturningType {
    #[default]
    Default,
    Old,
    New,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ParamKind {
    #[default]
    Extern,
    Exec,
    Sublink,
    Multiexpr,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum CoercionContext {
    #[default]
    Implicit,
    Assignment,
    Plpgsql,
    Explicit,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum CoercionForm {
    #[default]
    ExplicitCall,
    ExplicitCast,
    ImplicitCast,
    SqlSyntax,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum BoolExprType {
    #[default]
    AndExpr,
    OrExpr,
    NotExpr,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SubLinkType {
    #[default]
    ExistsSublink,
    AllSublink,
    AnySublink,
    RowcompareSublink,
    ExprSublink,
    MultiexprSublink,
    ArraySublink,
    CteSublink,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum MinMaxOp {
    #[default]
    Greatest,
    Least,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SqlValueFunctionOp {
    #[default]
    CurrentDate,
    CurrentTime,
    CurrentTimeN,
    CurrentTimestamp,
    CurrentTimestampN,
    Localtime,
    LocaltimeN,
    Localtimestamp,
    LocaltimestampN,
    CurrentRole,
    CurrentUser,
    User,
    SessionUser,
    CurrentCatalog,
    CurrentSchema,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum XmlExprOp {
    #[default]
    Xmlconcat,
    Xmlelement,
    Xmlforest,
    Xmlparse,
    Xmlpi,
    Xmlroot,
    Xmlserialize,
    Document,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum XmlOptionType {
    #[default]
    Document,
    Content,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonEncoding {
    #[default]
    Default,
    Utf8,
    Utf16,
    Utf32,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonFormatType {
    #[default]
    Default,
    Json,
    Jsonb,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonConstructorType {
    #[default]
    Object = 1,
    Array,
    ArrayQuery,
    Objectagg,
    Arrayagg,
    Parse,
    Scalar,
    Serialize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonValueType {
    #[default]
    Any,
    Object,
    Array,
    Scalar,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonWrapper {
    #[default]
    Unspec,
    None,
    Conditional,
    Unconditional,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonBehaviorType {
    #[default]
    Null = 0,
    Error,
    Empty,
    True,
    False,
    Unknown,
    EmptyArray,
    EmptyObject,
    Default,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonExprOp {
    #[default]
    ExistsOp,
    QueryOp,
    ValueOp,
    TableOp,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum NullTestType {
    #[default]
    Null,
    NotNull,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum BoolTestType {
    #[default]
    True,
    NotTrue,
    False,
    NotFalse,
    Unknown,
    NotUnknown,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum MergeMatchKind {
    #[default]
    Matched,
    NotMatchedBySource,
    NotMatchedByTarget,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum QuerySource {
    #[default]
    Original,
    Parser,
    InsteadRule,
    QualInsteadRule,
    NonInsteadRule,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SortByDir {
    #[default]
    Default,
    Asc,
    Desc,
    Using,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SortByNulls {
    #[default]
    Default,
    First,
    Last,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SetQuantifier {
    #[default]
    Default,
    All,
    Distinct,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AExprKind {
    #[default]
    Op,
    OpAny,
    OpAll,
    Distinct,
    NotDistinct,
    Nullif,
    In,
    Like,
    Ilike,
    Similar,
    Between,
    NotBetween,
    BetweenSym,
    NotBetweenSym,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum RoleSpecType {
    #[default]
    Cstring,
    CurrentRole,
    CurrentUser,
    SessionUser,
    Public,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum TableLikeOption {
    #[default]
    Comments = 1 << 0,
    Compression = 1 << 1,
    Constraints = 1 << 2,
    Defaults = 1 << 3,
    Generated = 1 << 4,
    Identity = 1 << 5,
    Indexes = 1 << 6,
    Statistics = 1 << 7,
    Storage = 1 << 8,
    All = i32::MAX as isize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum DefElemAction {
    #[default]
    Unspec,
    Set,
    Add,
    Drop,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum PartitionStrategy {
    #[default]
    List = 108,
    Range = 114,
    Hash = 104,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum PartitionRangeDatumKind {
    #[default]
    Minvalue = -1,
    Value = 0,
    Maxvalue = 1,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum GraphElementPatternKind {
    #[default]
    VertexPattern,
    EdgePatternLeft,
    EdgePatternRight,
    EdgePatternAny,
    ParenExpr,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum RteKind {
    #[default]
    Relation,
    Subquery,
    Join,
    Function,
    Tablefunc,
    Values,
    Cte,
    Namedtuplestore,
    GraphTable,
    Result,
    Group,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum WcoKind {
    #[default]
    ViewCheck,
    RlsInsertCheck,
    RlsUpdateCheck,
    RlsConflictCheck,
    RlsMergeUpdateCheck,
    RlsMergeDeleteCheck,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum GroupingSetKind {
    #[default]
    Empty,
    Simple,
    Rollup,
    Cube,
    Sets,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum CteMaterialize {
    #[default]
    Default,
    Always,
    Never,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ReturningOptionKind {
    #[default]
    Old,
    New,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonQuotes {
    #[default]
    Unspec,
    Keep,
    Omit,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum JsonTableColumnType {
    #[default]
    ForOrdinality,
    Regular,
    Exists,
    Formatted,
    Nested,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SetOperation {
    #[default]
    None = 0,
    Union,
    Intersect,
    Except,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ObjectType {
    #[default]
    AccessMethod,
    Aggregate,
    Amop,
    Amproc,
    Attribute,
    Cast,
    Column,
    Collation,
    Conversion,
    Database,
    Default,
    Defacl,
    Domain,
    Domconstraint,
    EventTrigger,
    Extension,
    Fdw,
    ForeignServer,
    ForeignTable,
    Function,
    Index,
    Language,
    Largeobject,
    Matview,
    Opclass,
    Operator,
    Opfamily,
    ParameterAcl,
    Policy,
    Procedure,
    Propgraph,
    Publication,
    PublicationNamespace,
    PublicationRel,
    Role,
    Routine,
    Rule,
    Schema,
    Sequence,
    Subscription,
    StatisticExt,
    Tabconstraint,
    Table,
    Tablespace,
    Transform,
    Trigger,
    Tsconfiguration,
    Tsdictionary,
    Tsparser,
    Tstemplate,
    Type,
    UserMapping,
    View,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum DropBehavior {
    #[default]
    Restrict,
    Cascade,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AlterTableType {
    #[default]
    AddColumn,
    AddColumnToView,
    ColumnDefault,
    CookedColumnDefault,
    DropNotNull,
    SetNotNull,
    SetExpression,
    DropExpression,
    SetStatistics,
    SetOptions,
    ResetOptions,
    SetStorage,
    SetCompression,
    DropColumn,
    AddIndex,
    ReAddIndex,
    AddConstraint,
    ReAddConstraint,
    ReAddDomainConstraint,
    AlterConstraint,
    ValidateConstraint,
    AddIndexConstraint,
    DropConstraint,
    ReAddComment,
    AlterColumnType,
    AlterColumnGenericOptions,
    ChangeOwner,
    ClusterOn,
    DropCluster,
    SetLogged,
    SetUnLogged,
    DropOids,
    SetAccessMethod,
    SetTableSpace,
    SetRelOptions,
    ResetRelOptions,
    ReplaceRelOptions,
    EnableTrig,
    EnableAlwaysTrig,
    EnableReplicaTrig,
    DisableTrig,
    EnableTrigAll,
    DisableTrigAll,
    EnableTrigUser,
    DisableTrigUser,
    EnableRule,
    EnableAlwaysRule,
    EnableReplicaRule,
    DisableRule,
    AddInherit,
    DropInherit,
    AddOf,
    DropOf,
    ReplicaIdentity,
    EnableRowSecurity,
    DisableRowSecurity,
    ForceRowSecurity,
    NoForceRowSecurity,
    GenericOptions,
    AttachPartition,
    DetachPartition,
    DetachPartitionFinalize,
    SplitPartition,
    MergePartitions,
    AddIdentity,
    SetIdentity,
    DropIdentity,
    ReAddStatistics,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AlterDomainType {
    #[default]
    AlterDefault = 84,
    DropNotNull = 78,
    SetNotNull = 79,
    AddConstraint = 67,
    DropConstraint = 88,
    ValidateConstraint = 86,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum GrantTargetType {
    #[default]
    Object,
    AllInSchema,
    Defaults,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum VariableSetKind {
    #[default]
    SetValue,
    SetDefault,
    SetCurrent,
    SetMulti,
    Reset,
    ResetAll,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ConstrType {
    #[default]
    Null,
    Notnull,
    Default,
    Identity,
    Generated,
    Check,
    Primary,
    Unique,
    Exclusion,
    Foreign,
    AttrDeferrable,
    AttrNotDeferrable,
    AttrDeferred,
    AttrImmediate,
    AttrEnforced,
    AttrNotEnforced,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ImportForeignSchemaType {
    #[default]
    All,
    LimitTo,
    Except,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum RoleStmtType {
    #[default]
    Role,
    User,
    Group,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum FetchDirection {
    #[default]
    Forward,
    Backward,
    Absolute,
    Relative,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum FetchDirectionKeywords {
    #[default]
    None = 0,
    Next,
    Prior,
    First,
    Last,
    Absolute,
    Relative,
    All,
    Forward,
    ForwardAll,
    Backward,
    BackwardAll,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum FunctionParameterMode {
    #[default]
    In = 105,
    Out = 111,
    Inout = 98,
    Variadic = 118,
    Table = 116,
    Default = 100,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum TransactionStmtKind {
    #[default]
    Begin,
    Start,
    Commit,
    Rollback,
    Savepoint,
    Release,
    RollbackTo,
    Prepare,
    CommitPrepared,
    RollbackPrepared,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ViewCheckOption {
    #[default]
    NoCheckOption,
    LocalCheckOption,
    CascadedCheckOption,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum RepackCommand {
    #[default]
    Cluster = 1,
    Repack,
    Vacuumfull,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum DiscardMode {
    #[default]
    All,
    Plans,
    Sequences,
    Temp,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ReindexObjectType {
    #[default]
    Index,
    Table,
    Schema,
    System,
    Database,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AlterPropGraphElementKind {
    #[default]
    Vertex = 1,
    Edge = 2,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AlterTsConfigType {
    #[default]
    AddMapping,
    AlterMappingForToken,
    ReplaceDict,
    ReplaceDictForToken,
    DropMapping,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum PublicationObjSpecType {
    #[default]
    Table,
    ExceptTable,
    TablesInSchema,
    TablesInCurSchema,
    Continuation,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum PublicationAllObjType {
    #[default]
    Tables,
    Sequences,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AlterPublicationAction {
    #[default]
    AddObjects,
    DropObjects,
    SetObjects,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum AlterSubscriptionType {
    #[default]
    Options,
    Server,
    Connection,
    SetPublication,
    AddPublication,
    DropPublication,
    RefreshPublication,
    RefreshSequences,
    Enabled,
    Skip,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValUnion {
    Integer(Integer),
    Float(Float),
    Boolean(Boolean),
    String(String),
    BitString(BitString),
}

impl Default for ValUnion {
    fn default() -> Self {
        Self::Integer(Integer::default())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Alias(Alias),
    RangeVar(RangeVar),
    TableFunc(TableFunc),
    IntoClause(IntoClause),
    Expr,
    Var(Var),
    Const(Const),
    Param(Param),
    Aggref(Aggref),
    GroupingFunc(GroupingFunc),
    WindowFunc(WindowFunc),
    WindowFuncRunCondition(WindowFuncRunCondition),
    MergeSupportFunc(MergeSupportFunc),
    SubscriptingRef(SubscriptingRef),
    FuncExpr(FuncExpr),
    NamedArgExpr(NamedArgExpr),
    OpExpr(OpExpr),
    ScalarArrayOpExpr(ScalarArrayOpExpr),
    BoolExpr(BoolExpr),
    SubLink(SubLink),
    SubPlan(SubPlan),
    AlternativeSubPlan(AlternativeSubPlan),
    FieldSelect(FieldSelect),
    FieldStore(FieldStore),
    RelabelType(RelabelType),
    CoerceViaIo(CoerceViaIo),
    ArrayCoerceExpr(ArrayCoerceExpr),
    ConvertRowtypeExpr(ConvertRowtypeExpr),
    CollateExpr(CollateExpr),
    CaseExpr(CaseExpr),
    CaseWhen(CaseWhen),
    CaseTestExpr(CaseTestExpr),
    ArrayExpr(ArrayExpr),
    RowExpr(RowExpr),
    RowCompareExpr(RowCompareExpr),
    CoalesceExpr(CoalesceExpr),
    MinMaxExpr(MinMaxExpr),
    SqlValueFunction(SqlValueFunction),
    XmlExpr(XmlExpr),
    JsonFormat(JsonFormat),
    JsonReturning(JsonReturning),
    JsonValueExpr(JsonValueExpr),
    JsonConstructorExpr(JsonConstructorExpr),
    JsonIsPredicate(JsonIsPredicate),
    JsonBehavior(JsonBehavior),
    JsonExpr(JsonExpr),
    JsonTablePath(JsonTablePath),
    JsonTablePlan,
    JsonTablePathScan(JsonTablePathScan),
    JsonTableSiblingJoin(JsonTableSiblingJoin),
    NullTest(NullTest),
    BooleanTest(BooleanTest),
    MergeAction(MergeAction),
    CoerceToDomain(CoerceToDomain),
    CoerceToDomainValue(CoerceToDomainValue),
    SetToDefault(SetToDefault),
    CurrentOfExpr(CurrentOfExpr),
    NextValueExpr(NextValueExpr),
    InferenceElem(InferenceElem),
    ReturningExpr(ReturningExpr),
    GraphLabelRef(GraphLabelRef),
    GraphPropertyRef(GraphPropertyRef),
    TargetEntry(TargetEntry),
    RangeTblRef(RangeTblRef),
    JoinExpr(JoinExpr),
    FromExpr(FromExpr),
    OnConflictExpr(OnConflictExpr),
    ForPortionOfExpr(ForPortionOfExpr),
    Query(Query),
    TypeName(TypeName),
    ColumnRef(ColumnRef),
    ParamRef(ParamRef),
    AExpr(AExpr),
    AConst(AConst),
    TypeCast(TypeCast),
    CollateClause(CollateClause),
    RoleSpec(RoleSpec),
    FuncCall(FuncCall),
    AStar,
    AIndices(AIndices),
    AIndirection(AIndirection),
    AArrayExpr(AArrayExpr),
    ResTarget(ResTarget),
    MultiAssignRef(MultiAssignRef),
    SortBy(SortBy),
    WindowDef(WindowDef),
    RangeSubselect(RangeSubselect),
    RangeFunction(RangeFunction),
    RangeTableFunc(RangeTableFunc),
    RangeTableFuncCol(RangeTableFuncCol),
    RangeGraphTable(RangeGraphTable),
    RangeTableSample(RangeTableSample),
    ColumnDef(ColumnDef),
    TableLikeClause(TableLikeClause),
    IndexElem(IndexElem),
    DefElem(DefElem),
    LockingClause(LockingClause),
    XmlSerialize(XmlSerialize),
    PartitionElem(PartitionElem),
    PartitionSpec(PartitionSpec),
    PartitionRangeDatum(PartitionRangeDatum),
    SinglePartitionSpec(SinglePartitionSpec),
    PartitionCmd(PartitionCmd),
    GraphPattern(GraphPattern),
    GraphElementPattern(GraphElementPattern),
    RangeTblEntry(RangeTblEntry),
    RtePermissionInfo(RtePermissionInfo),
    RangeTblFunction(RangeTblFunction),
    TableSampleClause(TableSampleClause),
    WithCheckOption(WithCheckOption),
    SortGroupClause(SortGroupClause),
    GroupingSet(GroupingSet),
    WindowClause(WindowClause),
    RowMarkClause(RowMarkClause),
    ForPortionOfClause(ForPortionOfClause),
    WithClause(WithClause),
    InferClause(InferClause),
    OnConflictClause(OnConflictClause),
    CteSearchClause(CteSearchClause),
    CteCycleClause(CteCycleClause),
    CommonTableExpr(CommonTableExpr),
    MergeWhenClause(MergeWhenClause),
    ReturningOption(ReturningOption),
    ReturningClause(ReturningClause),
    TriggerTransition(TriggerTransition),
    JsonOutput(JsonOutput),
    JsonArgument(JsonArgument),
    JsonFuncExpr(JsonFuncExpr),
    JsonTablePathSpec(JsonTablePathSpec),
    JsonTable(JsonTable),
    JsonTableColumn(JsonTableColumn),
    JsonKeyValue(JsonKeyValue),
    JsonParseExpr(JsonParseExpr),
    JsonScalarExpr(JsonScalarExpr),
    JsonSerializeExpr(JsonSerializeExpr),
    JsonObjectConstructor(JsonObjectConstructor),
    JsonArrayConstructor(JsonArrayConstructor),
    JsonArrayQueryConstructor(JsonArrayQueryConstructor),
    JsonAggConstructor(JsonAggConstructor),
    JsonObjectAgg(JsonObjectAgg),
    JsonArrayAgg(JsonArrayAgg),
    RawStmt(RawStmt),
    InsertStmt(InsertStmt),
    DeleteStmt(DeleteStmt),
    UpdateStmt(UpdateStmt),
    MergeStmt(MergeStmt),
    SelectStmt(SelectStmt),
    SetOperationStmt(SetOperationStmt),
    ReturnStmt(ReturnStmt),
    PlAssignStmt(PlAssignStmt),
    CreateSchemaStmt(CreateSchemaStmt),
    AlterTableStmt(AlterTableStmt),
    AlterTableCmd(AlterTableCmd),
    AtAlterConstraint(AtAlterConstraint),
    ReplicaIdentityStmt(ReplicaIdentityStmt),
    AlterCollationStmt(AlterCollationStmt),
    AlterDomainStmt(AlterDomainStmt),
    GrantStmt(GrantStmt),
    ObjectWithArgs(ObjectWithArgs),
    AccessPriv(AccessPriv),
    GrantRoleStmt(GrantRoleStmt),
    AlterDefaultPrivilegesStmt(AlterDefaultPrivilegesStmt),
    CopyStmt(CopyStmt),
    VariableSetStmt(VariableSetStmt),
    VariableShowStmt(VariableShowStmt),
    CreateStmt(CreateStmt),
    Constraint(Constraint),
    CreateTableSpaceStmt(CreateTableSpaceStmt),
    DropTableSpaceStmt(DropTableSpaceStmt),
    AlterTableSpaceOptionsStmt(AlterTableSpaceOptionsStmt),
    AlterTableMoveAllStmt(AlterTableMoveAllStmt),
    CreateExtensionStmt(CreateExtensionStmt),
    AlterExtensionStmt(AlterExtensionStmt),
    AlterExtensionContentsStmt(AlterExtensionContentsStmt),
    CreateFdwStmt(CreateFdwStmt),
    AlterFdwStmt(AlterFdwStmt),
    CreateForeignServerStmt(CreateForeignServerStmt),
    AlterForeignServerStmt(AlterForeignServerStmt),
    CreateForeignTableStmt(CreateForeignTableStmt),
    CreateUserMappingStmt(CreateUserMappingStmt),
    AlterUserMappingStmt(AlterUserMappingStmt),
    DropUserMappingStmt(DropUserMappingStmt),
    ImportForeignSchemaStmt(ImportForeignSchemaStmt),
    CreatePolicyStmt(CreatePolicyStmt),
    AlterPolicyStmt(AlterPolicyStmt),
    CreateAmStmt(CreateAmStmt),
    CreateTrigStmt(CreateTrigStmt),
    CreateEventTrigStmt(CreateEventTrigStmt),
    AlterEventTrigStmt(AlterEventTrigStmt),
    CreatePLangStmt(CreatePLangStmt),
    CreateRoleStmt(CreateRoleStmt),
    AlterRoleStmt(AlterRoleStmt),
    AlterRoleSetStmt(AlterRoleSetStmt),
    DropRoleStmt(DropRoleStmt),
    CreateSeqStmt(CreateSeqStmt),
    AlterSeqStmt(AlterSeqStmt),
    DefineStmt(DefineStmt),
    CreateDomainStmt(CreateDomainStmt),
    CreateOpClassStmt(CreateOpClassStmt),
    CreateOpClassItem(CreateOpClassItem),
    CreateOpFamilyStmt(CreateOpFamilyStmt),
    AlterOpFamilyStmt(AlterOpFamilyStmt),
    DropStmt(DropStmt),
    TruncateStmt(TruncateStmt),
    CommentStmt(CommentStmt),
    SecLabelStmt(SecLabelStmt),
    DeclareCursorStmt(DeclareCursorStmt),
    ClosePortalStmt(ClosePortalStmt),
    FetchStmt(FetchStmt),
    IndexStmt(IndexStmt),
    CreateStatsStmt(CreateStatsStmt),
    StatsElem(StatsElem),
    AlterStatsStmt(AlterStatsStmt),
    CreateFunctionStmt(CreateFunctionStmt),
    FunctionParameter(FunctionParameter),
    AlterFunctionStmt(AlterFunctionStmt),
    DoStmt(DoStmt),
    InlineCodeBlock(InlineCodeBlock),
    CallStmt(CallStmt),
    CallContext(CallContext),
    RenameStmt(RenameStmt),
    AlterObjectDependsStmt(AlterObjectDependsStmt),
    AlterObjectSchemaStmt(AlterObjectSchemaStmt),
    AlterOwnerStmt(AlterOwnerStmt),
    AlterOperatorStmt(AlterOperatorStmt),
    AlterTypeStmt(AlterTypeStmt),
    RuleStmt(RuleStmt),
    NotifyStmt(NotifyStmt),
    ListenStmt(ListenStmt),
    UnlistenStmt(UnlistenStmt),
    TransactionStmt(TransactionStmt),
    CompositeTypeStmt(CompositeTypeStmt),
    CreateEnumStmt(CreateEnumStmt),
    CreateRangeStmt(CreateRangeStmt),
    AlterEnumStmt(AlterEnumStmt),
    ViewStmt(ViewStmt),
    LoadStmt(LoadStmt),
    CreatedbStmt(CreatedbStmt),
    AlterDatabaseStmt(AlterDatabaseStmt),
    AlterDatabaseRefreshCollStmt(AlterDatabaseRefreshCollStmt),
    AlterDatabaseSetStmt(AlterDatabaseSetStmt),
    DropdbStmt(DropdbStmt),
    AlterSystemStmt(AlterSystemStmt),
    VacuumStmt(VacuumStmt),
    VacuumRelation(VacuumRelation),
    RepackStmt(RepackStmt),
    ExplainStmt(ExplainStmt),
    CreateTableAsStmt(CreateTableAsStmt),
    RefreshMatViewStmt(RefreshMatViewStmt),
    CheckPointStmt(CheckPointStmt),
    DiscardStmt(DiscardStmt),
    LockStmt(LockStmt),
    ConstraintsSetStmt(ConstraintsSetStmt),
    ReindexStmt(ReindexStmt),
    CreateConversionStmt(CreateConversionStmt),
    CreateCastStmt(CreateCastStmt),
    CreatePropGraphStmt(CreatePropGraphStmt),
    PropGraphVertex(PropGraphVertex),
    PropGraphEdge(PropGraphEdge),
    PropGraphLabelAndProperties(PropGraphLabelAndProperties),
    PropGraphProperties(PropGraphProperties),
    AlterPropGraphStmt(AlterPropGraphStmt),
    CreateTransformStmt(CreateTransformStmt),
    PrepareStmt(PrepareStmt),
    ExecuteStmt(ExecuteStmt),
    DeallocateStmt(DeallocateStmt),
    DropOwnedStmt(DropOwnedStmt),
    ReassignOwnedStmt(ReassignOwnedStmt),
    AlterTsDictionaryStmt(AlterTsDictionaryStmt),
    AlterTsConfigurationStmt(AlterTsConfigurationStmt),
    PublicationTable(PublicationTable),
    PublicationObjSpec(PublicationObjSpec),
    PublicationAllObjSpec(PublicationAllObjSpec),
    CreatePublicationStmt(CreatePublicationStmt),
    AlterPublicationStmt(AlterPublicationStmt),
    CreateSubscriptionStmt(CreateSubscriptionStmt),
    AlterSubscriptionStmt(AlterSubscriptionStmt),
    DropSubscriptionStmt(DropSubscriptionStmt),
    WaitStmt(WaitStmt),
    PartitionBoundSpec(PartitionBoundSpec),
    Integer(Integer),
    Float(Float),
    Boolean(Boolean),
    String(String),
    BitString(BitString),
    DistinctExpr(OpExpr),
    NullIfExpr(OpExpr),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Alias {
    pub aliasname: Option<std::string::String>,
    pub colnames: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeVar {
    pub catalogname: Option<std::string::String>,
    pub schemaname: Option<std::string::String>,
    pub relname: Option<std::string::String>,
    pub inh: bool,
    pub relpersistence: u8,
    pub alias: Option<Box<Alias>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableFunc {
    pub functype: TableFuncType,
    pub ns_uris: NodeList,
    pub ns_names: NodeList,
    pub docexpr: Option<Box<Node>>,
    pub rowexpr: Option<Box<Node>>,
    pub colnames: NodeList,
    pub coltypes: NodeList,
    pub coltypmods: NodeList,
    pub colcollations: NodeList,
    pub colexprs: NodeList,
    pub coldefexprs: NodeList,
    pub colvalexprs: NodeList,
    pub passingvalexprs: NodeList,
    pub notnulls: Option<Bitmapset>,
    pub plan: Option<Box<Node>>,
    pub ordinalitycol: i32,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IntoClause {
    pub rel: Option<Box<RangeVar>>,
    pub col_names: NodeList,
    pub access_method: Option<std::string::String>,
    pub options: NodeList,
    pub on_commit: OnCommitAction,
    pub table_space_name: Option<std::string::String>,
    pub view_query: Option<Box<Query>>,
    pub skip_data: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Expr {}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Var {
    pub varno: i32,
    pub varattno: AttrNumber,
    pub vartype: Oid,
    pub vartypmod: i32,
    pub varcollid: Oid,
    pub varnullingrels: Option<Bitmapset>,
    pub varlevelsup: Index,
    pub varreturningtype: VarReturningType,
    pub varnosyn: Index,
    pub varattnosyn: AttrNumber,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Const {
    pub consttype: Oid,
    pub consttypmod: i32,
    pub constcollid: Oid,
    pub constlen: i32,
    pub constvalue: Datum,
    pub constisnull: bool,
    pub constbyval: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Param {
    pub paramkind: ParamKind,
    pub paramid: i32,
    pub paramtype: Oid,
    pub paramtypmod: i32,
    pub paramcollid: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aggref {
    pub aggfnoid: Oid,
    pub aggtype: Oid,
    pub aggcollid: Oid,
    pub inputcollid: Oid,
    pub aggtranstype: Oid,
    pub aggargtypes: NodeList,
    pub aggdirectargs: NodeList,
    pub args: NodeList,
    pub aggorder: NodeList,
    pub aggdistinct: NodeList,
    pub aggfilter: Option<Box<Expr>>,
    pub aggstar: bool,
    pub aggvariadic: bool,
    pub aggkind: u8,
    pub aggpresorted: bool,
    pub agglevelsup: Index,
    pub aggsplit: AggSplit,
    pub aggno: i32,
    pub aggtransno: i32,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupingFunc {
    pub args: NodeList,
    pub refs: NodeList,
    pub cols: NodeList,
    pub agglevelsup: Index,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowFunc {
    pub winfnoid: Oid,
    pub wintype: Oid,
    pub wincollid: Oid,
    pub inputcollid: Oid,
    pub args: NodeList,
    pub aggfilter: Option<Box<Expr>>,
    pub run_condition: NodeList,
    pub winref: Index,
    pub winstar: bool,
    pub winagg: bool,
    pub ignore_nulls: i32,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowFuncRunCondition {
    pub opno: Oid,
    pub inputcollid: Oid,
    pub wfunc_left: bool,
    pub arg: Option<Box<Expr>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeSupportFunc {
    pub msftype: Oid,
    pub msfcollid: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubscriptingRef {
    pub refcontainertype: Oid,
    pub refelemtype: Oid,
    pub refrestype: Oid,
    pub reftypmod: i32,
    pub refcollid: Oid,
    pub refupperindexpr: NodeList,
    pub reflowerindexpr: NodeList,
    pub refexpr: Option<Box<Expr>>,
    pub refassgnexpr: Option<Box<Expr>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FuncExpr {
    pub funcid: Oid,
    pub funcresulttype: Oid,
    pub funcretset: bool,
    pub funcvariadic: bool,
    pub funcformat: CoercionForm,
    pub funccollid: Oid,
    pub inputcollid: Oid,
    pub args: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NamedArgExpr {
    pub arg: Option<Box<Node>>,
    pub name: Option<std::string::String>,
    pub argnumber: i32,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpExpr {
    pub opno: Oid,
    pub opfuncid: Oid,
    pub opresulttype: Oid,
    pub opretset: bool,
    pub opcollid: Oid,
    pub inputcollid: Oid,
    pub args: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScalarArrayOpExpr {
    pub opno: Oid,
    pub opfuncid: Oid,
    pub hashfuncid: Oid,
    pub negfuncid: Oid,
    pub use_or: bool,
    pub inputcollid: Oid,
    pub args: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoolExpr {
    pub boolop: BoolExprType,
    pub args: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubLink {
    pub sub_link_type: SubLinkType,
    pub sub_link_id: i32,
    pub testexpr: Option<Box<Node>>,
    pub oper_name: NodeList,
    pub subselect: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubPlan {
    pub sub_link_type: SubLinkType,
    pub testexpr: Option<Box<Node>>,
    pub param_ids: NodeList,
    pub plan_id: i32,
    pub plan_name: Option<std::string::String>,
    pub first_col_type: Oid,
    pub first_col_typmod: i32,
    pub first_col_collation: Oid,
    pub is_init_plan: bool,
    pub use_hash_table: bool,
    pub unknown_eq_false: bool,
    pub parallel_safe: bool,
    pub set_param: NodeList,
    pub par_param: NodeList,
    pub args: NodeList,
    pub disabled_nodes: i32,
    pub startup_cost: Cost,
    pub per_call_cost: Cost,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlternativeSubPlan {
    pub subplans: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldSelect {
    pub arg: Option<Box<Expr>>,
    pub fieldnum: AttrNumber,
    pub resulttype: Oid,
    pub resulttypmod: i32,
    pub resultcollid: Oid,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldStore {
    pub arg: Option<Box<Expr>>,
    pub newvals: NodeList,
    pub fieldnums: NodeList,
    pub resulttype: Oid,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RelabelType {
    pub arg: Option<Box<Expr>>,
    pub resulttype: Oid,
    pub resulttypmod: i32,
    pub resultcollid: Oid,
    pub relabelformat: CoercionForm,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoerceViaIo {
    pub arg: Option<Box<Expr>>,
    pub resulttype: Oid,
    pub resultcollid: Oid,
    pub coerceformat: CoercionForm,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArrayCoerceExpr {
    pub arg: Option<Box<Expr>>,
    pub elemexpr: Option<Box<Expr>>,
    pub resulttype: Oid,
    pub resulttypmod: i32,
    pub resultcollid: Oid,
    pub coerceformat: CoercionForm,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConvertRowtypeExpr {
    pub arg: Option<Box<Expr>>,
    pub resulttype: Oid,
    pub convertformat: CoercionForm,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollateExpr {
    pub arg: Option<Box<Expr>>,
    pub coll_oid: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaseExpr {
    pub casetype: Oid,
    pub casecollid: Oid,
    pub arg: Option<Box<Node>>,
    pub args: NodeList,
    pub defresult: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaseWhen {
    pub expr: Option<Box<Node>>,
    pub result: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaseTestExpr {
    pub type_id: Oid,
    pub type_mod: i32,
    pub collation: Oid,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArrayExpr {
    pub array_typeid: Oid,
    pub array_collid: Oid,
    pub element_typeid: Oid,
    pub elements: NodeList,
    pub multidims: bool,
    pub list_start: ParseLoc,
    pub list_end: ParseLoc,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowExpr {
    pub args: NodeList,
    pub row_typeid: Oid,
    pub row_format: CoercionForm,
    pub colnames: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowCompareExpr {
    pub cmptype: CompareType,
    pub opnos: NodeList,
    pub opfamilies: NodeList,
    pub inputcollids: NodeList,
    pub largs: NodeList,
    pub rargs: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoalesceExpr {
    pub coalescetype: Oid,
    pub coalescecollid: Oid,
    pub args: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MinMaxExpr {
    pub minmaxtype: Oid,
    pub minmaxcollid: Oid,
    pub inputcollid: Oid,
    pub op: MinMaxOp,
    pub args: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SqlValueFunction {
    pub op: SqlValueFunctionOp,
    pub type_oid: Oid,
    pub typmod: i32,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct XmlExpr {
    pub op: XmlExprOp,
    pub name: Option<std::string::String>,
    pub named_args: NodeList,
    pub arg_names: NodeList,
    pub args: NodeList,
    pub xmloption: XmlOptionType,
    pub indent: bool,
    pub type_oid: Oid,
    pub typmod: i32,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonFormat {
    pub format_type: JsonFormatType,
    pub encoding: JsonEncoding,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonReturning {
    pub format: Option<Box<JsonFormat>>,
    pub typid: Oid,
    pub typmod: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonValueExpr {
    pub raw_expr: Option<Box<Node>>,
    pub formatted_expr: Option<Box<Node>>,
    pub format: Option<Box<JsonFormat>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonConstructorExpr {
    pub constructor_type: JsonConstructorType,
    pub args: NodeList,
    pub func: Option<Box<Expr>>,
    pub coercion: Option<Box<Expr>>,
    pub returning: Option<Box<JsonReturning>>,
    pub orig_query: Option<Box<Node>>,
    pub absent_on_null: bool,
    pub unique: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonIsPredicate {
    pub expr: Option<Box<Node>>,
    pub format: Option<Box<JsonFormat>>,
    pub item_type: JsonValueType,
    pub unique_keys: bool,
    pub expr_base_type: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonBehavior {
    pub btype: JsonBehaviorType,
    pub expr: Option<Box<Node>>,
    pub coerce: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonExpr {
    pub op: JsonExprOp,
    pub column_name: Option<std::string::String>,
    pub formatted_expr: Option<Box<Node>>,
    pub format: Option<Box<JsonFormat>>,
    pub path_spec: Option<Box<Node>>,
    pub returning: Option<Box<JsonReturning>>,
    pub passing_names: NodeList,
    pub passing_values: NodeList,
    pub on_empty: Option<Box<JsonBehavior>>,
    pub on_error: Option<Box<JsonBehavior>>,
    pub use_io_coercion: bool,
    pub use_json_coercion: bool,
    pub wrapper: JsonWrapper,
    pub omit_quotes: bool,
    pub collation: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonTablePath {
    pub value: Option<Box<Const>>,
    pub name: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonTablePlan {}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonTablePathScan {
    pub plan: JsonTablePlan,
    pub path: Option<Box<JsonTablePath>>,
    pub error_on_error: bool,
    pub child: Option<Box<JsonTablePlan>>,
    pub col_min: i32,
    pub col_max: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonTableSiblingJoin {
    pub plan: JsonTablePlan,
    pub lplan: Option<Box<JsonTablePlan>>,
    pub rplan: Option<Box<JsonTablePlan>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NullTest {
    pub arg: Option<Box<Node>>,
    pub nulltesttype: NullTestType,
    pub argisrow: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BooleanTest {
    pub arg: Option<Box<Node>>,
    pub booltesttype: BoolTestType,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeAction {
    pub match_kind: MergeMatchKind,
    pub command_type: CmdType,
    pub override_: OverridingKind,
    pub qual: Option<Box<Node>>,
    pub target_list: NodeList,
    pub update_colnos: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoerceToDomain {
    pub arg: Option<Box<Expr>>,
    pub resulttype: Oid,
    pub resulttypmod: i32,
    pub resultcollid: Oid,
    pub coercionformat: CoercionForm,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoerceToDomainValue {
    pub type_id: Oid,
    pub type_mod: i32,
    pub collation: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetToDefault {
    pub type_id: Oid,
    pub type_mod: i32,
    pub collation: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurrentOfExpr {
    pub cvarno: Index,
    pub cursor_name: Option<std::string::String>,
    pub cursor_param: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NextValueExpr {
    pub seqid: Oid,
    pub type_id: Oid,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InferenceElem {
    pub expr: Option<Box<Node>>,
    pub infercollid: Oid,
    pub inferopclass: Oid,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReturningExpr {
    pub retlevelsup: i32,
    pub retold: bool,
    pub retexpr: Option<Box<Expr>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphLabelRef {
    pub labelid: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphPropertyRef {
    pub elvarname: Option<std::string::String>,
    pub propid: Oid,
    pub type_id: Oid,
    pub typmod: i32,
    pub collation: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TargetEntry {
    pub expr: Option<Box<Expr>>,
    pub resno: AttrNumber,
    pub resname: Option<std::string::String>,
    pub ressortgroupref: Index,
    pub resorigtbl: Oid,
    pub resorigcol: AttrNumber,
    pub resjunk: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeTblRef {
    pub rtindex: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JoinExpr {
    pub jointype: JoinType,
    pub is_natural: bool,
    pub larg: Option<Box<Node>>,
    pub rarg: Option<Box<Node>>,
    pub using_clause: NodeList,
    pub join_using_alias: Option<Box<Alias>>,
    pub quals: Option<Box<Node>>,
    pub alias: Option<Box<Alias>>,
    pub rtindex: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FromExpr {
    pub fromlist: NodeList,
    pub quals: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnConflictExpr {
    pub action: OnConflictAction,
    pub arbiter_elems: NodeList,
    pub arbiter_where: Option<Box<Node>>,
    pub constraint: Oid,
    pub lock_strength: LockClauseStrength,
    pub on_conflict_set: NodeList,
    pub on_conflict_where: Option<Box<Node>>,
    pub excl_rel_index: i32,
    pub excl_rel_tlist: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForPortionOfExpr {
    pub range_var: Option<Box<Var>>,
    pub range_name: Option<std::string::String>,
    pub target_from: Option<Box<Node>>,
    pub target_to: Option<Box<Node>>,
    pub target_range: Option<Box<Node>>,
    pub range_type: Oid,
    pub is_domain: bool,
    pub overlaps_expr: Option<Box<Node>>,
    pub range_target_list: NodeList,
    pub without_portion_proc: Oid,
    pub location: ParseLoc,
    pub target_location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub command_type: CmdType,
    pub query_source: QuerySource,
    pub can_set_tag: bool,
    pub utility_stmt: Option<Box<Node>>,
    pub result_relation: i32,
    pub for_portion_of: Option<Box<ForPortionOfExpr>>,
    pub has_aggs: bool,
    pub has_window_funcs: bool,
    pub has_target_sr_fs: bool,
    pub has_sub_links: bool,
    pub has_distinct_on: bool,
    pub has_recursive: bool,
    pub has_modifying_cte: bool,
    pub has_for_update: bool,
    pub has_row_security: bool,
    pub has_group_rte: bool,
    pub is_return: bool,
    pub cte_list: NodeList,
    pub rtable: NodeList,
    pub rteperminfos: NodeList,
    pub jointree: Option<Box<FromExpr>>,
    pub merge_action_list: NodeList,
    pub merge_target_relation: i32,
    pub merge_join_condition: Option<Box<Node>>,
    pub target_list: NodeList,
    pub override_: OverridingKind,
    pub on_conflict: Option<Box<OnConflictExpr>>,
    pub returning_old_alias: Option<std::string::String>,
    pub returning_new_alias: Option<std::string::String>,
    pub returning_list: NodeList,
    pub group_clause: NodeList,
    pub group_distinct: bool,
    pub group_by_all: bool,
    pub grouping_sets: NodeList,
    pub having_qual: Option<Box<Node>>,
    pub window_clause: NodeList,
    pub distinct_clause: NodeList,
    pub sort_clause: NodeList,
    pub limit_offset: Option<Box<Node>>,
    pub limit_count: Option<Box<Node>>,
    pub limit_option: LimitOption,
    pub row_marks: NodeList,
    pub set_operations: Option<Box<Node>>,
    pub constraint_deps: NodeList,
    pub with_check_options: NodeList,
    pub stmt_location: ParseLoc,
    pub stmt_len: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeName {
    pub names: NodeList,
    pub type_oid: Oid,
    pub setof: bool,
    pub pct_type: bool,
    pub typmods: NodeList,
    pub typemod: i32,
    pub array_bounds: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColumnRef {
    pub fields: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParamRef {
    pub number: i32,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AExpr {
    pub kind: AExprKind,
    pub name: NodeList,
    pub lexpr: Option<Box<Node>>,
    pub rexpr: Option<Box<Node>>,
    pub rexpr_list_start: ParseLoc,
    pub rexpr_list_end: ParseLoc,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AConst {
    pub val: ValUnion,
    pub isnull: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeCast {
    pub arg: Option<Box<Node>>,
    pub type_name: Option<Box<TypeName>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollateClause {
    pub arg: Option<Box<Node>>,
    pub collname: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoleSpec {
    pub roletype: RoleSpecType,
    pub rolename: Option<std::string::String>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FuncCall {
    pub funcname: NodeList,
    pub args: NodeList,
    pub agg_order: NodeList,
    pub agg_filter: Option<Box<Node>>,
    pub over: Option<Box<WindowDef>>,
    pub ignore_nulls: i32,
    pub agg_within_group: bool,
    pub agg_star: bool,
    pub agg_distinct: bool,
    pub func_variadic: bool,
    pub funcformat: CoercionForm,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AIndices {
    pub is_slice: bool,
    pub lidx: Option<Box<Node>>,
    pub uidx: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AIndirection {
    pub arg: Option<Box<Node>>,
    pub indirection: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AArrayExpr {
    pub elements: NodeList,
    pub list_start: ParseLoc,
    pub list_end: ParseLoc,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResTarget {
    pub name: Option<std::string::String>,
    pub indirection: NodeList,
    pub val: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MultiAssignRef {
    pub source: Option<Box<Node>>,
    pub colno: i32,
    pub ncolumns: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortBy {
    pub node: Option<Box<Node>>,
    pub sortby_dir: SortByDir,
    pub sortby_nulls: SortByNulls,
    pub use_op: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowDef {
    pub name: Option<std::string::String>,
    pub refname: Option<std::string::String>,
    pub partition_clause: NodeList,
    pub order_clause: NodeList,
    pub frame_options: i32,
    pub start_offset: Option<Box<Node>>,
    pub end_offset: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeSubselect {
    pub lateral: bool,
    pub subquery: Option<Box<Node>>,
    pub alias: Option<Box<Alias>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeFunction {
    pub lateral: bool,
    pub ordinality: bool,
    pub is_rowsfrom: bool,
    pub functions: NodeList,
    pub alias: Option<Box<Alias>>,
    pub coldeflist: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeTableFunc {
    pub lateral: bool,
    pub docexpr: Option<Box<Node>>,
    pub rowexpr: Option<Box<Node>>,
    pub namespaces: NodeList,
    pub columns: NodeList,
    pub alias: Option<Box<Alias>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeTableFuncCol {
    pub colname: Option<std::string::String>,
    pub type_name: Option<Box<TypeName>>,
    pub for_ordinality: bool,
    pub is_not_null: bool,
    pub colexpr: Option<Box<Node>>,
    pub coldefexpr: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeGraphTable {
    pub graph_name: Option<Box<RangeVar>>,
    pub graph_pattern: Option<Box<GraphPattern>>,
    pub columns: NodeList,
    pub alias: Option<Box<Alias>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeTableSample {
    pub relation: Option<Box<Node>>,
    pub method: NodeList,
    pub args: NodeList,
    pub repeatable: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColumnDef {
    pub colname: Option<std::string::String>,
    pub type_name: Option<Box<TypeName>>,
    pub compression: Option<std::string::String>,
    pub inhcount: i16,
    pub is_local: bool,
    pub is_not_null: bool,
    pub is_from_type: bool,
    pub storage: u8,
    pub storage_name: Option<std::string::String>,
    pub raw_default: Option<Box<Node>>,
    pub cooked_default: Option<Box<Node>>,
    pub identity: u8,
    pub identity_sequence: Option<Box<RangeVar>>,
    pub generated: u8,
    pub coll_clause: Option<Box<CollateClause>>,
    pub coll_oid: Oid,
    pub constraints: NodeList,
    pub fdwoptions: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableLikeClause {
    pub relation: Option<Box<RangeVar>>,
    pub options: u32,
    pub relation_oid: Oid,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndexElem {
    pub name: Option<std::string::String>,
    pub expr: Option<Box<Node>>,
    pub indexcolname: Option<std::string::String>,
    pub collation: NodeList,
    pub opclass: NodeList,
    pub opclassopts: NodeList,
    pub ordering: SortByDir,
    pub nulls_ordering: SortByNulls,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DefElem {
    pub defnamespace: Option<std::string::String>,
    pub defname: Option<std::string::String>,
    pub arg: Option<Box<Node>>,
    pub defaction: DefElemAction,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LockingClause {
    pub locked_rels: NodeList,
    pub strength: LockClauseStrength,
    pub wait_policy: LockWaitPolicy,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct XmlSerialize {
    pub xmloption: XmlOptionType,
    pub expr: Option<Box<Node>>,
    pub type_name: Option<Box<TypeName>>,
    pub indent: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartitionElem {
    pub name: Option<std::string::String>,
    pub expr: Option<Box<Node>>,
    pub collation: NodeList,
    pub opclass: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartitionSpec {
    pub strategy: PartitionStrategy,
    pub part_params: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartitionRangeDatum {
    pub kind: PartitionRangeDatumKind,
    pub value: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SinglePartitionSpec {
    pub name: Option<Box<RangeVar>>,
    pub bound: Option<Box<PartitionBoundSpec>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartitionCmd {
    pub name: Option<Box<RangeVar>>,
    pub bound: Option<Box<PartitionBoundSpec>>,
    pub partlist: NodeList,
    pub concurrent: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphPattern {
    pub path_pattern_list: NodeList,
    pub where_clause: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphElementPattern {
    pub kind: GraphElementPatternKind,
    pub variable: Option<std::string::String>,
    pub labelexpr: Option<Box<Node>>,
    pub subexpr: NodeList,
    pub where_clause: Option<Box<Node>>,
    pub quantifier: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeTblEntry {
    pub alias: Option<Box<Alias>>,
    pub eref: Option<Box<Alias>>,
    pub rtekind: RteKind,
    pub relid: Oid,
    pub inh: bool,
    pub relkind: u8,
    pub rellockmode: i32,
    pub perminfoindex: Index,
    pub tablesample: Option<Box<TableSampleClause>>,
    pub subquery: Option<Box<Query>>,
    pub security_barrier: bool,
    pub jointype: JoinType,
    pub joinmergedcols: i32,
    pub joinaliasvars: NodeList,
    pub joinleftcols: NodeList,
    pub joinrightcols: NodeList,
    pub join_using_alias: Option<Box<Alias>>,
    pub functions: NodeList,
    pub funcordinality: bool,
    pub tablefunc: Option<Box<TableFunc>>,
    pub graph_pattern: Option<Box<GraphPattern>>,
    pub graph_table_columns: NodeList,
    pub values_lists: NodeList,
    pub ctename: Option<std::string::String>,
    pub ctelevelsup: Index,
    pub self_reference: bool,
    pub coltypes: NodeList,
    pub coltypmods: NodeList,
    pub colcollations: NodeList,
    pub enrname: Option<std::string::String>,
    pub enrtuples: Cardinality,
    pub groupexprs: NodeList,
    pub lateral: bool,
    pub in_from_cl: bool,
    pub security_quals: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RtePermissionInfo {
    pub relid: Oid,
    pub inh: bool,
    pub required_perms: AclMode,
    pub check_as_user: Oid,
    pub selected_cols: Option<Bitmapset>,
    pub inserted_cols: Option<Bitmapset>,
    pub updated_cols: Option<Bitmapset>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeTblFunction {
    pub funcexpr: Option<Box<Node>>,
    pub funccolcount: i32,
    pub funccolnames: NodeList,
    pub funccoltypes: NodeList,
    pub funccoltypmods: NodeList,
    pub funccolcollations: NodeList,
    pub funcparams: Option<Bitmapset>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableSampleClause {
    pub tsmhandler: Oid,
    pub args: NodeList,
    pub repeatable: Option<Box<Expr>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WithCheckOption {
    pub kind: WcoKind,
    pub relname: Option<std::string::String>,
    pub polname: Option<std::string::String>,
    pub qual: Option<Box<Node>>,
    pub cascaded: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortGroupClause {
    pub tle_sort_group_ref: Index,
    pub eqop: Oid,
    pub sortop: Oid,
    pub reverse_sort: bool,
    pub nulls_first: bool,
    pub hashable: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupingSet {
    pub kind: GroupingSetKind,
    pub content: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowClause {
    pub name: Option<std::string::String>,
    pub refname: Option<std::string::String>,
    pub partition_clause: NodeList,
    pub order_clause: NodeList,
    pub frame_options: i32,
    pub start_offset: Option<Box<Node>>,
    pub end_offset: Option<Box<Node>>,
    pub start_in_range_func: Oid,
    pub end_in_range_func: Oid,
    pub in_range_coll: Oid,
    pub in_range_asc: bool,
    pub in_range_nulls_first: bool,
    pub winref: Index,
    pub copied_order: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowMarkClause {
    pub rti: Index,
    pub strength: LockClauseStrength,
    pub wait_policy: LockWaitPolicy,
    pub pushed_down: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForPortionOfClause {
    pub range_name: Option<std::string::String>,
    pub location: ParseLoc,
    pub target_location: ParseLoc,
    pub target: Option<Box<Node>>,
    pub target_start: Option<Box<Node>>,
    pub target_end: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WithClause {
    pub ctes: NodeList,
    pub recursive: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InferClause {
    pub index_elems: NodeList,
    pub where_clause: Option<Box<Node>>,
    pub conname: Option<std::string::String>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnConflictClause {
    pub action: OnConflictAction,
    pub infer: Option<Box<InferClause>>,
    pub lock_strength: LockClauseStrength,
    pub target_list: NodeList,
    pub where_clause: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CteSearchClause {
    pub search_col_list: NodeList,
    pub search_breadth_first: bool,
    pub search_seq_column: Option<std::string::String>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CteCycleClause {
    pub cycle_col_list: NodeList,
    pub cycle_mark_column: Option<std::string::String>,
    pub cycle_mark_value: Option<Box<Node>>,
    pub cycle_mark_default: Option<Box<Node>>,
    pub cycle_path_column: Option<std::string::String>,
    pub location: ParseLoc,
    pub cycle_mark_type: Oid,
    pub cycle_mark_typmod: i32,
    pub cycle_mark_collation: Oid,
    pub cycle_mark_neop: Oid,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommonTableExpr {
    pub ctename: Option<std::string::String>,
    pub aliascolnames: NodeList,
    pub ctematerialized: CteMaterialize,
    pub ctequery: Option<Box<Node>>,
    pub search_clause: Option<Box<CteSearchClause>>,
    pub cycle_clause: Option<Box<CteCycleClause>>,
    pub location: ParseLoc,
    pub cterecursive: bool,
    pub cterefcount: i32,
    pub ctecolnames: NodeList,
    pub ctecoltypes: NodeList,
    pub ctecoltypmods: NodeList,
    pub ctecolcollations: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeWhenClause {
    pub match_kind: MergeMatchKind,
    pub command_type: CmdType,
    pub override_: OverridingKind,
    pub condition: Option<Box<Node>>,
    pub target_list: NodeList,
    pub values: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReturningOption {
    pub option: ReturningOptionKind,
    pub value: Option<std::string::String>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReturningClause {
    pub options: NodeList,
    pub exprs: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriggerTransition {
    pub name: Option<std::string::String>,
    pub is_new: bool,
    pub is_table: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonOutput {
    pub type_name: Option<Box<TypeName>>,
    pub returning: Option<Box<JsonReturning>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonArgument {
    pub val: Option<Box<JsonValueExpr>>,
    pub name: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonFuncExpr {
    pub op: JsonExprOp,
    pub column_name: Option<std::string::String>,
    pub context_item: Option<Box<JsonValueExpr>>,
    pub pathspec: Option<Box<Node>>,
    pub passing: NodeList,
    pub output: Option<Box<JsonOutput>>,
    pub on_empty: Option<Box<JsonBehavior>>,
    pub on_error: Option<Box<JsonBehavior>>,
    pub wrapper: JsonWrapper,
    pub quotes: JsonQuotes,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonTablePathSpec {
    pub string: Option<Box<Node>>,
    pub name: Option<std::string::String>,
    pub name_location: ParseLoc,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonTable {
    pub context_item: Option<Box<JsonValueExpr>>,
    pub pathspec: Option<Box<JsonTablePathSpec>>,
    pub passing: NodeList,
    pub columns: NodeList,
    pub on_error: Option<Box<JsonBehavior>>,
    pub alias: Option<Box<Alias>>,
    pub lateral: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonTableColumn {
    pub coltype: JsonTableColumnType,
    pub name: Option<std::string::String>,
    pub type_name: Option<Box<TypeName>>,
    pub pathspec: Option<Box<JsonTablePathSpec>>,
    pub format: Option<Box<JsonFormat>>,
    pub wrapper: JsonWrapper,
    pub quotes: JsonQuotes,
    pub columns: NodeList,
    pub on_empty: Option<Box<JsonBehavior>>,
    pub on_error: Option<Box<JsonBehavior>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonKeyValue {
    pub key: Option<Box<Node>>,
    pub value: Option<Box<JsonValueExpr>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonParseExpr {
    pub expr: Option<Box<JsonValueExpr>>,
    pub output: Option<Box<JsonOutput>>,
    pub unique_keys: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonScalarExpr {
    pub expr: Option<Box<Node>>,
    pub output: Option<Box<JsonOutput>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonSerializeExpr {
    pub expr: Option<Box<JsonValueExpr>>,
    pub output: Option<Box<JsonOutput>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonObjectConstructor {
    pub exprs: NodeList,
    pub output: Option<Box<JsonOutput>>,
    pub absent_on_null: bool,
    pub unique: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonArrayConstructor {
    pub exprs: NodeList,
    pub output: Option<Box<JsonOutput>>,
    pub absent_on_null: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonArrayQueryConstructor {
    pub query: Option<Box<Node>>,
    pub output: Option<Box<JsonOutput>>,
    pub format: Option<Box<JsonFormat>>,
    pub absent_on_null: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonAggConstructor {
    pub output: Option<Box<JsonOutput>>,
    pub agg_filter: Option<Box<Node>>,
    pub agg_order: NodeList,
    pub over: Option<Box<WindowDef>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonObjectAgg {
    pub constructor: Option<Box<JsonAggConstructor>>,
    pub arg: Option<Box<JsonKeyValue>>,
    pub absent_on_null: bool,
    pub unique: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsonArrayAgg {
    pub constructor: Option<Box<JsonAggConstructor>>,
    pub arg: Option<Box<JsonValueExpr>>,
    pub absent_on_null: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawStmt {
    pub stmt: Option<Box<Node>>,
    pub stmt_location: ParseLoc,
    pub stmt_len: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InsertStmt {
    pub relation: Option<Box<RangeVar>>,
    pub cols: NodeList,
    pub select_stmt: Option<Box<Node>>,
    pub on_conflict_clause: Option<Box<OnConflictClause>>,
    pub returning_clause: Option<Box<ReturningClause>>,
    pub with_clause: Option<Box<WithClause>>,
    pub override_: OverridingKind,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeleteStmt {
    pub relation: Option<Box<RangeVar>>,
    pub using_clause: NodeList,
    pub where_clause: Option<Box<Node>>,
    pub returning_clause: Option<Box<ReturningClause>>,
    pub with_clause: Option<Box<WithClause>>,
    pub for_portion_of: Option<Box<ForPortionOfClause>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateStmt {
    pub relation: Option<Box<RangeVar>>,
    pub target_list: NodeList,
    pub where_clause: Option<Box<Node>>,
    pub from_clause: NodeList,
    pub returning_clause: Option<Box<ReturningClause>>,
    pub with_clause: Option<Box<WithClause>>,
    pub for_portion_of: Option<Box<ForPortionOfClause>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeStmt {
    pub relation: Option<Box<RangeVar>>,
    pub source_relation: Option<Box<Node>>,
    pub join_condition: Option<Box<Node>>,
    pub merge_when_clauses: NodeList,
    pub returning_clause: Option<Box<ReturningClause>>,
    pub with_clause: Option<Box<WithClause>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectStmt {
    pub distinct_clause: NodeList,
    pub into_clause: Option<Box<IntoClause>>,
    pub target_list: NodeList,
    pub from_clause: NodeList,
    pub where_clause: Option<Box<Node>>,
    pub group_clause: NodeList,
    pub group_distinct: bool,
    pub group_by_all: bool,
    pub having_clause: Option<Box<Node>>,
    pub window_clause: NodeList,
    pub values_lists: NodeList,
    pub sort_clause: NodeList,
    pub limit_offset: Option<Box<Node>>,
    pub limit_count: Option<Box<Node>>,
    pub limit_option: LimitOption,
    pub locking_clause: NodeList,
    pub with_clause: Option<Box<WithClause>>,
    pub op: SetOperation,
    pub all: bool,
    pub larg: Option<Box<SelectStmt>>,
    pub rarg: Option<Box<SelectStmt>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetOperationStmt {
    pub op: SetOperation,
    pub all: bool,
    pub larg: Option<Box<Node>>,
    pub rarg: Option<Box<Node>>,
    pub col_types: NodeList,
    pub col_typmods: NodeList,
    pub col_collations: NodeList,
    pub group_clauses: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReturnStmt {
    pub returnval: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlAssignStmt {
    pub name: Option<std::string::String>,
    pub indirection: NodeList,
    pub nnames: i32,
    pub val: Option<Box<SelectStmt>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateSchemaStmt {
    pub schemaname: Option<std::string::String>,
    pub authrole: Option<Box<RoleSpec>>,
    pub schema_elts: NodeList,
    pub if_not_exists: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterTableStmt {
    pub relation: Option<Box<RangeVar>>,
    pub cmds: NodeList,
    pub objtype: ObjectType,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterTableCmd {
    pub subtype: AlterTableType,
    pub name: Option<std::string::String>,
    pub num: i16,
    pub newowner: Option<Box<RoleSpec>>,
    pub def: Option<Box<Node>>,
    pub behavior: DropBehavior,
    pub missing_ok: bool,
    pub recurse: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AtAlterConstraint {
    pub conname: Option<std::string::String>,
    pub alter_enforceability: bool,
    pub is_enforced: bool,
    pub alter_deferrability: bool,
    pub deferrable: bool,
    pub initdeferred: bool,
    pub alter_inheritability: bool,
    pub noinherit: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplicaIdentityStmt {
    pub identity_type: u8,
    pub name: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterCollationStmt {
    pub collname: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterDomainStmt {
    pub subtype: AlterDomainType,
    pub type_name: NodeList,
    pub name: Option<std::string::String>,
    pub def: Option<Box<Node>>,
    pub behavior: DropBehavior,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GrantStmt {
    pub is_grant: bool,
    pub targtype: GrantTargetType,
    pub objtype: ObjectType,
    pub objects: NodeList,
    pub privileges: NodeList,
    pub grantees: NodeList,
    pub grant_option: bool,
    pub grantor: Option<Box<RoleSpec>>,
    pub behavior: DropBehavior,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectWithArgs {
    pub objname: NodeList,
    pub objargs: Vec<Option<Node>>,
    pub objfuncargs: NodeList,
    pub args_unspecified: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccessPriv {
    pub priv_name: Option<std::string::String>,
    pub cols: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GrantRoleStmt {
    pub granted_roles: NodeList,
    pub grantee_roles: NodeList,
    pub is_grant: bool,
    pub opt: NodeList,
    pub grantor: Option<Box<RoleSpec>>,
    pub behavior: DropBehavior,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterDefaultPrivilegesStmt {
    pub options: NodeList,
    pub action: Option<Box<GrantStmt>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CopyStmt {
    pub relation: Option<Box<RangeVar>>,
    pub query: Option<Box<Node>>,
    pub attlist: NodeList,
    pub is_from: bool,
    pub is_program: bool,
    pub filename: Option<std::string::String>,
    pub options: NodeList,
    pub where_clause: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VariableSetStmt {
    pub kind: VariableSetKind,
    pub name: Option<std::string::String>,
    pub args: NodeList,
    pub jumble_args: bool,
    pub is_local: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VariableShowStmt {
    pub name: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateStmt {
    pub relation: Option<Box<RangeVar>>,
    pub table_elts: NodeList,
    pub inh_relations: NodeList,
    pub partbound: Option<Box<PartitionBoundSpec>>,
    pub partspec: Option<Box<PartitionSpec>>,
    pub of_typename: Option<Box<TypeName>>,
    pub constraints: NodeList,
    pub nnconstraints: NodeList,
    pub options: NodeList,
    pub oncommit: OnCommitAction,
    pub tablespacename: Option<std::string::String>,
    pub access_method: Option<std::string::String>,
    pub if_not_exists: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Constraint {
    pub contype: ConstrType,
    pub conname: Option<std::string::String>,
    pub deferrable: bool,
    pub initdeferred: bool,
    pub is_enforced: bool,
    pub skip_validation: bool,
    pub initially_valid: bool,
    pub is_no_inherit: bool,
    pub raw_expr: Option<Box<Node>>,
    pub cooked_expr: Option<std::string::String>,
    pub generated_when: u8,
    pub generated_kind: u8,
    pub nulls_not_distinct: bool,
    pub keys: NodeList,
    pub without_overlaps: bool,
    pub including: NodeList,
    pub exclusions: NodeList,
    pub options: NodeList,
    pub indexname: Option<std::string::String>,
    pub indexspace: Option<std::string::String>,
    pub reset_default_tblspc: bool,
    pub access_method: Option<std::string::String>,
    pub where_clause: Option<Box<Node>>,
    pub pktable: Option<Box<RangeVar>>,
    pub fk_attrs: NodeList,
    pub pk_attrs: NodeList,
    pub fk_with_period: bool,
    pub pk_with_period: bool,
    pub fk_matchtype: u8,
    pub fk_upd_action: u8,
    pub fk_del_action: u8,
    pub fk_del_set_cols: NodeList,
    pub old_conpfeqop: NodeList,
    pub old_pktable_oid: Oid,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateTableSpaceStmt {
    pub tablespacename: Option<std::string::String>,
    pub owner: Option<Box<RoleSpec>>,
    pub location: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropTableSpaceStmt {
    pub tablespacename: Option<std::string::String>,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterTableSpaceOptionsStmt {
    pub tablespacename: Option<std::string::String>,
    pub options: NodeList,
    pub is_reset: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterTableMoveAllStmt {
    pub orig_tablespacename: Option<std::string::String>,
    pub objtype: ObjectType,
    pub roles: NodeList,
    pub new_tablespacename: Option<std::string::String>,
    pub nowait: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateExtensionStmt {
    pub extname: Option<std::string::String>,
    pub if_not_exists: bool,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterExtensionStmt {
    pub extname: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterExtensionContentsStmt {
    pub extname: Option<std::string::String>,
    pub action: i32,
    pub objtype: ObjectType,
    pub object: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateFdwStmt {
    pub fdwname: Option<std::string::String>,
    pub func_options: NodeList,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterFdwStmt {
    pub fdwname: Option<std::string::String>,
    pub func_options: NodeList,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateForeignServerStmt {
    pub servername: Option<std::string::String>,
    pub servertype: Option<std::string::String>,
    pub version: Option<std::string::String>,
    pub fdwname: Option<std::string::String>,
    pub if_not_exists: bool,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterForeignServerStmt {
    pub servername: Option<std::string::String>,
    pub version: Option<std::string::String>,
    pub options: NodeList,
    pub has_version: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateForeignTableStmt {
    pub base: CreateStmt,
    pub servername: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateUserMappingStmt {
    pub user: Option<Box<RoleSpec>>,
    pub servername: Option<std::string::String>,
    pub if_not_exists: bool,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterUserMappingStmt {
    pub user: Option<Box<RoleSpec>>,
    pub servername: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropUserMappingStmt {
    pub user: Option<Box<RoleSpec>>,
    pub servername: Option<std::string::String>,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportForeignSchemaStmt {
    pub server_name: Option<std::string::String>,
    pub remote_schema: Option<std::string::String>,
    pub local_schema: Option<std::string::String>,
    pub list_type: ImportForeignSchemaType,
    pub table_list: NodeList,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreatePolicyStmt {
    pub policy_name: Option<std::string::String>,
    pub table: Option<Box<RangeVar>>,
    pub cmd_name: Option<std::string::String>,
    pub permissive: bool,
    pub roles: NodeList,
    pub qual: Option<Box<Node>>,
    pub with_check: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterPolicyStmt {
    pub policy_name: Option<std::string::String>,
    pub table: Option<Box<RangeVar>>,
    pub roles: NodeList,
    pub qual: Option<Box<Node>>,
    pub with_check: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateAmStmt {
    pub amname: Option<std::string::String>,
    pub handler_name: NodeList,
    pub amtype: u8,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateTrigStmt {
    pub replace: bool,
    pub isconstraint: bool,
    pub trigname: Option<std::string::String>,
    pub relation: Option<Box<RangeVar>>,
    pub funcname: NodeList,
    pub args: NodeList,
    pub row: bool,
    pub timing: i16,
    pub events: i16,
    pub columns: NodeList,
    pub when_clause: Option<Box<Node>>,
    pub transition_rels: NodeList,
    pub deferrable: bool,
    pub initdeferred: bool,
    pub constrrel: Option<Box<RangeVar>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateEventTrigStmt {
    pub trigname: Option<std::string::String>,
    pub eventname: Option<std::string::String>,
    pub whenclause: NodeList,
    pub funcname: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterEventTrigStmt {
    pub trigname: Option<std::string::String>,
    pub tgenabled: u8,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreatePLangStmt {
    pub replace: bool,
    pub plname: Option<std::string::String>,
    pub plhandler: NodeList,
    pub plinline: NodeList,
    pub plvalidator: NodeList,
    pub pltrusted: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateRoleStmt {
    pub stmt_type: RoleStmtType,
    pub role: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterRoleStmt {
    pub role: Option<Box<RoleSpec>>,
    pub options: NodeList,
    pub action: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterRoleSetStmt {
    pub role: Option<Box<RoleSpec>>,
    pub database: Option<std::string::String>,
    pub setstmt: Option<Box<VariableSetStmt>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropRoleStmt {
    pub roles: NodeList,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateSeqStmt {
    pub sequence: Option<Box<RangeVar>>,
    pub options: NodeList,
    pub owner_id: Oid,
    pub for_identity: bool,
    pub if_not_exists: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterSeqStmt {
    pub sequence: Option<Box<RangeVar>>,
    pub options: NodeList,
    pub for_identity: bool,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DefineStmt {
    pub kind: ObjectType,
    pub oldstyle: bool,
    pub defnames: NodeList,
    pub args: NodeList,
    pub definition: NodeList,
    pub if_not_exists: bool,
    pub replace: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateDomainStmt {
    pub domainname: NodeList,
    pub type_name: Option<Box<TypeName>>,
    pub coll_clause: Option<Box<CollateClause>>,
    pub constraints: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateOpClassStmt {
    pub opclassname: NodeList,
    pub opfamilyname: NodeList,
    pub amname: Option<std::string::String>,
    pub datatype: Option<Box<TypeName>>,
    pub items: NodeList,
    pub is_default: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateOpClassItem {
    pub itemtype: i32,
    pub name: Option<Box<ObjectWithArgs>>,
    pub number: i32,
    pub order_family: NodeList,
    pub class_args: NodeList,
    pub storedtype: Option<Box<TypeName>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateOpFamilyStmt {
    pub opfamilyname: NodeList,
    pub amname: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterOpFamilyStmt {
    pub opfamilyname: NodeList,
    pub amname: Option<std::string::String>,
    pub is_drop: bool,
    pub items: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropStmt {
    pub objects: NodeList,
    pub remove_type: ObjectType,
    pub behavior: DropBehavior,
    pub missing_ok: bool,
    pub concurrent: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TruncateStmt {
    pub relations: NodeList,
    pub restart_seqs: bool,
    pub behavior: DropBehavior,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommentStmt {
    pub objtype: ObjectType,
    pub object: Option<Box<Node>>,
    pub comment: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecLabelStmt {
    pub objtype: ObjectType,
    pub object: Option<Box<Node>>,
    pub provider: Option<std::string::String>,
    pub label: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeclareCursorStmt {
    pub portalname: Option<std::string::String>,
    pub options: i32,
    pub query: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClosePortalStmt {
    pub portalname: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FetchStmt {
    pub direction: FetchDirection,
    pub how_many: i64,
    pub portalname: Option<std::string::String>,
    pub ismove: bool,
    pub direction_keyword: FetchDirectionKeywords,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndexStmt {
    pub idxname: Option<std::string::String>,
    pub relation: Option<Box<RangeVar>>,
    pub access_method: Option<std::string::String>,
    pub table_space: Option<std::string::String>,
    pub index_params: NodeList,
    pub index_including_params: NodeList,
    pub options: NodeList,
    pub where_clause: Option<Box<Node>>,
    pub exclude_op_names: NodeList,
    pub idxcomment: Option<std::string::String>,
    pub index_oid: Oid,
    pub old_number: RelFileNumber,
    pub old_create_subid: SubTransactionId,
    pub old_first_relfilelocator_subid: SubTransactionId,
    pub unique: bool,
    pub nulls_not_distinct: bool,
    pub primary: bool,
    pub isconstraint: bool,
    pub iswithoutoverlaps: bool,
    pub deferrable: bool,
    pub initdeferred: bool,
    pub transformed: bool,
    pub concurrent: bool,
    pub if_not_exists: bool,
    pub reset_default_tblspc: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateStatsStmt {
    pub defnames: NodeList,
    pub stat_types: NodeList,
    pub exprs: NodeList,
    pub relations: NodeList,
    pub stxcomment: Option<std::string::String>,
    pub transformed: bool,
    pub if_not_exists: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatsElem {
    pub name: Option<std::string::String>,
    pub expr: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterStatsStmt {
    pub defnames: NodeList,
    pub stxstattarget: Option<Box<Node>>,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateFunctionStmt {
    pub is_procedure: bool,
    pub replace: bool,
    pub funcname: NodeList,
    pub parameters: NodeList,
    pub return_type: Option<Box<TypeName>>,
    pub options: NodeList,
    pub sql_body: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FunctionParameter {
    pub name: Option<std::string::String>,
    pub arg_type: Option<Box<TypeName>>,
    pub mode: FunctionParameterMode,
    pub defexpr: Option<Box<Node>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterFunctionStmt {
    pub objtype: ObjectType,
    pub func: Option<Box<ObjectWithArgs>>,
    pub actions: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DoStmt {
    pub args: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineCodeBlock {
    pub source_text: Option<std::string::String>,
    pub lang_oid: Oid,
    pub lang_is_trusted: bool,
    pub atomic: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallStmt {
    pub funccall: Option<Box<FuncCall>>,
    pub funcexpr: Option<Box<FuncExpr>>,
    pub outargs: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallContext {
    pub atomic: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenameStmt {
    pub rename_type: ObjectType,
    pub relation_type: ObjectType,
    pub relation: Option<Box<RangeVar>>,
    pub object: Option<Box<Node>>,
    pub subname: Option<std::string::String>,
    pub newname: Option<std::string::String>,
    pub behavior: DropBehavior,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterObjectDependsStmt {
    pub object_type: ObjectType,
    pub relation: Option<Box<RangeVar>>,
    pub object: Option<Box<Node>>,
    pub extname: Option<Box<String>>,
    pub remove: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterObjectSchemaStmt {
    pub object_type: ObjectType,
    pub relation: Option<Box<RangeVar>>,
    pub object: Option<Box<Node>>,
    pub newschema: Option<std::string::String>,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterOwnerStmt {
    pub object_type: ObjectType,
    pub relation: Option<Box<RangeVar>>,
    pub object: Option<Box<Node>>,
    pub newowner: Option<Box<RoleSpec>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterOperatorStmt {
    pub opername: Option<Box<ObjectWithArgs>>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterTypeStmt {
    pub type_name: NodeList,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleStmt {
    pub relation: Option<Box<RangeVar>>,
    pub rulename: Option<std::string::String>,
    pub where_clause: Option<Box<Node>>,
    pub event: CmdType,
    pub instead: bool,
    pub actions: NodeList,
    pub replace: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NotifyStmt {
    pub conditionname: Option<std::string::String>,
    pub payload: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListenStmt {
    pub conditionname: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnlistenStmt {
    pub conditionname: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransactionStmt {
    pub kind: TransactionStmtKind,
    pub options: NodeList,
    pub savepoint_name: Option<std::string::String>,
    pub gid: Option<std::string::String>,
    pub chain: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompositeTypeStmt {
    pub typevar: Option<Box<RangeVar>>,
    pub coldeflist: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateEnumStmt {
    pub type_name: NodeList,
    pub vals: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateRangeStmt {
    pub type_name: NodeList,
    pub params: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterEnumStmt {
    pub type_name: NodeList,
    pub old_val: Option<std::string::String>,
    pub new_val: Option<std::string::String>,
    pub new_val_neighbor: Option<std::string::String>,
    pub new_val_is_after: bool,
    pub skip_if_new_val_exists: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewStmt {
    pub view: Option<Box<RangeVar>>,
    pub aliases: NodeList,
    pub query: Option<Box<Node>>,
    pub replace: bool,
    pub options: NodeList,
    pub with_check_option: ViewCheckOption,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadStmt {
    pub filename: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreatedbStmt {
    pub dbname: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterDatabaseStmt {
    pub dbname: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterDatabaseRefreshCollStmt {
    pub dbname: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterDatabaseSetStmt {
    pub dbname: Option<std::string::String>,
    pub setstmt: Option<Box<VariableSetStmt>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropdbStmt {
    pub dbname: Option<std::string::String>,
    pub missing_ok: bool,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterSystemStmt {
    pub setstmt: Option<Box<VariableSetStmt>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VacuumStmt {
    pub options: NodeList,
    pub rels: NodeList,
    pub is_vacuumcmd: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VacuumRelation {
    pub relation: Option<Box<RangeVar>>,
    pub oid: Oid,
    pub va_cols: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepackStmt {
    pub command: RepackCommand,
    pub relation: Option<Box<VacuumRelation>>,
    pub indexname: Option<std::string::String>,
    pub usingindex: bool,
    pub params: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExplainStmt {
    pub query: Option<Box<Node>>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateTableAsStmt {
    pub query: Option<Box<Node>>,
    pub into: Option<Box<IntoClause>>,
    pub objtype: ObjectType,
    pub is_select_into: bool,
    pub if_not_exists: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefreshMatViewStmt {
    pub concurrent: bool,
    pub skip_data: bool,
    pub relation: Option<Box<RangeVar>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CheckPointStmt {
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiscardStmt {
    pub target: DiscardMode,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LockStmt {
    pub relations: NodeList,
    pub mode: i32,
    pub nowait: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConstraintsSetStmt {
    pub constraints: NodeList,
    pub deferred: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReindexStmt {
    pub kind: ReindexObjectType,
    pub relation: Option<Box<RangeVar>>,
    pub name: Option<std::string::String>,
    pub params: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateConversionStmt {
    pub conversion_name: NodeList,
    pub for_encoding_name: Option<std::string::String>,
    pub to_encoding_name: Option<std::string::String>,
    pub func_name: NodeList,
    pub def: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateCastStmt {
    pub sourcetype: Option<Box<TypeName>>,
    pub targettype: Option<Box<TypeName>>,
    pub func: Option<Box<ObjectWithArgs>>,
    pub context: CoercionContext,
    pub inout: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreatePropGraphStmt {
    pub pgname: Option<Box<RangeVar>>,
    pub vertex_tables: NodeList,
    pub edge_tables: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropGraphVertex {
    pub vtable: Option<Box<RangeVar>>,
    pub vkey: NodeList,
    pub labels: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropGraphEdge {
    pub etable: Option<Box<RangeVar>>,
    pub ekey: NodeList,
    pub esrckey: NodeList,
    pub esrcvertex: Option<std::string::String>,
    pub esrcvertexcols: NodeList,
    pub edestkey: NodeList,
    pub edestvertex: Option<std::string::String>,
    pub edestvertexcols: NodeList,
    pub labels: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropGraphLabelAndProperties {
    pub label: Option<std::string::String>,
    pub properties: Option<Box<PropGraphProperties>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropGraphProperties {
    pub properties: NodeList,
    pub all: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterPropGraphStmt {
    pub pgname: Option<Box<RangeVar>>,
    pub missing_ok: bool,
    pub add_vertex_tables: NodeList,
    pub add_edge_tables: NodeList,
    pub drop_vertex_tables: NodeList,
    pub drop_edge_tables: NodeList,
    pub drop_behavior: DropBehavior,
    pub element_kind: AlterPropGraphElementKind,
    pub element_alias: Option<std::string::String>,
    pub add_labels: NodeList,
    pub drop_label: Option<std::string::String>,
    pub alter_label: Option<std::string::String>,
    pub add_properties: Option<Box<PropGraphProperties>>,
    pub drop_properties: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateTransformStmt {
    pub replace: bool,
    pub type_name: Option<Box<TypeName>>,
    pub lang: Option<std::string::String>,
    pub fromsql: Option<Box<ObjectWithArgs>>,
    pub tosql: Option<Box<ObjectWithArgs>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrepareStmt {
    pub name: Option<std::string::String>,
    pub argtypes: NodeList,
    pub query: Option<Box<Node>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExecuteStmt {
    pub name: Option<std::string::String>,
    pub params: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeallocateStmt {
    pub name: Option<std::string::String>,
    pub isall: bool,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropOwnedStmt {
    pub roles: NodeList,
    pub behavior: DropBehavior,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReassignOwnedStmt {
    pub roles: NodeList,
    pub newrole: Option<Box<RoleSpec>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterTsDictionaryStmt {
    pub dictname: NodeList,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterTsConfigurationStmt {
    pub kind: AlterTsConfigType,
    pub cfgname: NodeList,
    pub tokentype: NodeList,
    pub dicts: NodeList,
    pub override_: bool,
    pub replace: bool,
    pub missing_ok: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PublicationTable {
    pub relation: Option<Box<RangeVar>>,
    pub where_clause: Option<Box<Node>>,
    pub columns: NodeList,
    pub except: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PublicationObjSpec {
    pub pubobjtype: PublicationObjSpecType,
    pub name: Option<std::string::String>,
    pub pubtable: Option<Box<PublicationTable>>,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PublicationAllObjSpec {
    pub pubobjtype: PublicationAllObjType,
    pub except_tables: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreatePublicationStmt {
    pub pubname: Option<std::string::String>,
    pub options: NodeList,
    pub pubobjects: NodeList,
    pub for_all_tables: bool,
    pub for_all_sequences: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterPublicationStmt {
    pub pubname: Option<std::string::String>,
    pub options: NodeList,
    pub pubobjects: NodeList,
    pub action: AlterPublicationAction,
    pub for_all_tables: bool,
    pub for_all_sequences: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateSubscriptionStmt {
    pub subname: Option<std::string::String>,
    pub servername: Option<std::string::String>,
    pub conninfo: Option<std::string::String>,
    pub publication: NodeList,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlterSubscriptionStmt {
    pub kind: AlterSubscriptionType,
    pub subname: Option<std::string::String>,
    pub servername: Option<std::string::String>,
    pub conninfo: Option<std::string::String>,
    pub publication: NodeList,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropSubscriptionStmt {
    pub subname: Option<std::string::String>,
    pub missing_ok: bool,
    pub behavior: DropBehavior,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WaitStmt {
    pub lsn_literal: Option<std::string::String>,
    pub options: NodeList,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartitionBoundSpec {
    pub strategy: u8,
    pub is_default: bool,
    pub modulus: i32,
    pub remainder: i32,
    pub listdatums: NodeList,
    pub lowerdatums: NodeList,
    pub upperdatums: NodeList,
    pub location: ParseLoc,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Integer {
    pub ival: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Float {
    pub fval: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Boolean {
    pub boolval: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct String {
    pub sval: Option<std::string::String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BitString {
    pub bsval: Option<std::string::String>,
}
pub type DistinctExpr = OpExpr;
pub type NullIfExpr = OpExpr;

impl Integer {
    pub fn new(ival: i32) -> Self {
        Self { ival }
    }
}

impl Float {
    pub fn new(fval: impl Into<std::string::String>) -> Self {
        Self {
            fval: Some(fval.into()),
        }
    }
}

impl Boolean {
    pub fn new(boolval: bool) -> Self {
        Self { boolval }
    }
}

impl String {
    pub fn new(sval: impl Into<std::string::String>) -> Self {
        Self {
            sval: Some(sval.into()),
        }
    }
}

impl BitString {
    pub fn new(bsval: impl Into<std::string::String>) -> Self {
        Self {
            bsval: Some(bsval.into()),
        }
    }
}

impl AConst {
    pub fn null(location: ParseLoc) -> Self {
        Self {
            isnull: true,
            location,
            ..Self::default()
        }
    }

    pub fn integer(ival: i32, location: ParseLoc) -> Self {
        Self {
            val: ValUnion::Integer(Integer::new(ival)),
            isnull: false,
            location,
        }
    }

    pub fn string(sval: impl Into<std::string::String>, location: ParseLoc) -> Self {
        Self {
            val: ValUnion::String(String::new(sval)),
            isnull: false,
            location,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_stmt_can_wrap_statement_node() {
        let select = Node::SelectStmt(SelectStmt {
            ..SelectStmt::default()
        });
        let raw = RawStmt {
            stmt: Some(Box::new(select)),
            stmt_location: 0,
            stmt_len: 8,
        };

        assert!(matches!(raw.stmt.as_deref(), Some(Node::SelectStmt(_))));
    }

    #[test]
    fn a_const_literal_constructors_preserve_value_nodes() {
        let literal = AConst::string("postgres", 7);

        assert_eq!(literal.location, 7);
        assert!(!literal.isnull);
        assert_eq!(
            literal.val,
            ValUnion::String(String {
                sval: Some("postgres".to_owned()),
            })
        );
    }
}
