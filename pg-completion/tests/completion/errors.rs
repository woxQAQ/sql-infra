use pg_completion::{CompletionError, CompletionRequest, complete};
use pg_parser::TextSize;

#[test]
fn public_error_wrapper_preserves_parser_cursor_errors() {
    let error = complete(
        CompletionRequest {
            sql: "",
            cursor: TextSize::new(1),
            search_path: &[],
        },
        None,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CompletionError::Syntax(pg_parser::CompletionError::CursorOutOfBounds { .. })
    ));
}
