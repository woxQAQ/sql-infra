# Separate PostgreSQL locations from text ranges

PostgreSQL raw-node `location` fields remain semantic grammar anchors for raw-tree fidelity, while tooling uses half-open UTF-8 `TextRange` values carried by tokens, errors, and parsed-statement metadata. We rejected redefining or extending every raw AST location into a node span because that would couple PostgreSQL-compatible trees to diagnostics and completion needs, blur operator anchors with source coverage, and still fail to provide a lossless syntax representation.
