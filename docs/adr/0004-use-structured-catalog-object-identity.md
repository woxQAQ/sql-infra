# Use structured catalog object identity

Completion catalog results use one precise PostgreSQL catalog object kind and a structured identity containing namespace, owning object, and kind-specific signature where applicable. Display labels and details are derived presentation data rather than identity; this keeps overloads and same-named objects distinct while allowing `Catalog` to remain a single deep search interface.
