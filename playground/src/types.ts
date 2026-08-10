export interface OffsetRange {
  start: number;
  end: number;
}

export interface SourceRange {
  utf8: OffsetRange;
  utf16: OffsetRange;
}

export interface CompletionItemDto {
  label: string;
  insertText: string;
  replacementRange: OffsetRange;
  kind: string;
  objectKind?: string;
  detail: string;
  origin: string;
  sortText: string;
  triggerSuggest: boolean;
}

export interface NamePartDto {
  text: string;
  normalized: string;
  quoted: boolean;
  range: SourceRange;
}

export interface RelationDto {
  kind: string;
  name: NamePartDto[];
  alias?: NamePartDto;
  explicitColumns: NamePartDto[];
  qualifiedOnly: boolean;
  syntaxRange: SourceRange;
  bodyRange?: SourceRange;
  lateral: boolean;
  unsupported?: {
    reason: string;
    range: SourceRange;
  };
}

export interface ContextDto {
  point: {
    requestedUtf16: number;
    effectiveUtf16: number;
    utf8: number;
    adjusted: boolean;
  };
  statementRange: SourceRange;
  replacementRange: SourceRange;
  prefix: {
    raw: string;
    normalized: string;
    quoting: string;
  };
  expectations: {
    tokens: string[];
    directTokens: string[];
    lookaheadTokens: string[];
    expressionStartTokens: string[];
    expressionContinuationTokens: string[];
    followTokens: string[];
    phrases: string[];
    slots: string[];
  };
  intent: {
    objectKinds: string[];
    qualifier: NamePartDto[];
    membership?: {
      memberKinds: string[];
      owner: {
        objectKinds: string[];
        name: NamePartDto[];
      };
    };
  };
  scope: {
    local: RelationDto[];
    outer: RelationDto[][];
    ctes: Array<{
      name: NamePartDto;
      explicitColumns: NamePartDto[];
      syntaxRange: SourceRange;
      bodyRange: SourceRange;
    }>;
    dmlTarget?: RelationDto;
    mergeSource?: RelationDto;
  };
  diagnostics: Array<{
    kind: string;
    range: SourceRange;
  }>;
}

export interface CompletionResponseDto {
  items: CompletionItemDto[];
  context: ContextDto;
}

export interface WireResponse {
  ok: boolean;
  completion?: CompletionResponseDto;
  error?: string;
}

export interface CatalogDocument {
  searchPath?: string[];
  objects?: Array<{
    kind: string;
    name: string[];
    detail?: string;
    members?: Array<{
      kind: string;
      name: string;
      detail?: string;
    }>;
  }>;
}

export interface WorkerRequest {
  id: number;
  source: string;
  cursorUtf16: number;
  catalog: CatalogDocument;
}

export interface WorkerResponse {
  id: number;
  elapsedMs: number;
  response: WireResponse;
}
