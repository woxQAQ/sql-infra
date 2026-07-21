use pg_completion::{CompletionRequest, complete};
use pg_parser::TextSize;

#[test]
fn public_interface_preserves_parser_cursor_errors() {
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
        &error,
        pg_parser::CompletionError::CursorOutOfBounds { .. }
    ));
    assert_eq!(
        error.to_string(),
        "completion cursor 1 is beyond source length 0"
    );
}
