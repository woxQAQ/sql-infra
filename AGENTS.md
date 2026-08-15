# AGENTS.md

## Project overview

A rust Monorepo that provides unified SQL infra including parse SQL statements, analyse, diagnostics, completion and so on.

We have implemented a hand-written recursive descent parser that turns a PostgreSQL text into a AST. The project is on the greenfield stage, breaking changes are free, do not concern about compatibility.We are going to implement more dialects' parser and build advanced features base on parsers.

## Hints

./CONTEXT.md is helpful for you before dig into the repo

You are not allowed to use the words: delve, landscape, tapestry, robust, seam, seamless, cutting-edge, transformative, pioneering, leverage
