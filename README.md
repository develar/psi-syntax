# psi-syntax

Lossless lexers and parsers in Rust, ported method by method from the ones behind IntelliJ's PSI and from Roslyn,
whose trees print exactly what the originals print. Crates of [accio](../README.md)'s Cargo workspace, which reads
them (`crates/extract`); Branch Deck depends on them as a revision of github.com/develar/psi-syntax, the public
mirror of this directory (`../scripts/psi-syntax-mirror.sh`):

- `psi-builder`: the language-neutral half, IntelliJ's `PsiBuilder` (markers, production, edge binders), the preorder
  column tree and the parse driver that expands lazily parsed elements in place;
- `kotlin-syntax`: Kotlin, ported from the Kotlin compiler's own (`Kotlin.flex`, `KotlinParsing`,
  `KotlinExpressionParsing`, KDoc), its tree `DebugUtil.psiToString`'s;
- `java-syntax`: Java, ported from IntelliJ's `java-syntax`: the Java and JavaDoc lexers and the parsers, equal to
  IntelliJ's Java PSI on every file tested;
- `csharp-syntax`: C#, ported from Roslyn 5.9.0 (`Lexer`, `DirectiveParser`, `LanguageParser`,
  `DocumentationCommentParser`): Roslyn's syntax tree with its trivia, missing tokens and diagnostics, equal to
  Roslyn's on dotnet/roslyn, dotnet/runtime, dotnet/aspnetcore, Avalonia and Roslyn's own parser tests, and its
  syntax node properties as getters over the tree (`syntax_getters`: a function per field of every node type, the
  abstract classes' and the partial classes' computed ones), equal to Roslyn's own on every node of the same inputs;
- `starlark-syntax`: Starlark from the IntelliJ Bazel plugin (`Starlark.flex`, the indenting lexer and parser), with
  `BUILD`/`.bzl`/module `FileKind`s and a `DebugUtil.psiToString`-compatible dump;
- `ksdump`: dumps, benchmarks and lexes listed files like the oracles.

See AGENTS.md for the porting rules and the oracles, docs/DESIGN.md for the architecture and its measurements.

```rust
use kotlin_syntax::{FileKind, ParseOptions, Parser};

let mut parser = Parser::new(); // one per thread; reuses its buffers from file to file
let tree = parser.parse(b"fun f() = 1", FileKind::Kt); // Tree: Send + Sync, ids in preorder
for (token, range) in tree.leaves() { /* exact highlighting: soft keywords and ?. ?: !! as the parser sees them */ }
let decls = parser.parse_with_options(b"fun f() { g() }", FileKind::Kt, ParseOptions { expand_bodies: false });
```

```rust
use csharp_syntax::{FileKind, LanguageVersion, Parser};

let mut parser = Parser::new(); // one per thread, as for Kotlin and Java
let tree = parser.parse(b"class C { void M() { } }", FileKind::default()); // LanguageVersion.Latest
let old = parser.parse(b"class C { int x = 1_000; }", FileKind::file(LanguageVersion::CSharp6));
for d in old.diagnostics() { /* d.id() "CS8059", d.start..d.end in bytes, "Feature 'digit separators' is not ..." */ }
// Roslyn's syntax node properties: ClassDeclarationSyntax.Identifier, TypeDeclarationSyntax.Members, ...
use csharp_syntax::syntax_getters::{class_declaration, type_declaration};
for class in tree.descendants(tree.root()).filter(|&n| tree.node_kind(n) == Some(csharp_syntax::NodeKind::ClassDeclaration)) {
    let name = class_declaration::identifier(&tree, class); // the `C` token: tree.value_text(id) is "C"
    let members = type_declaration::members(&tree, class).count(); // 1
}
```

```rust
use starlark_syntax::{FileKind, Parser};

let mut parser = Parser::new();
let file_kind = FileKind::from_path("pkg/BUILD.bazel").unwrap();
let tree = parser.parse(b"cc_library()", file_kind);
let dump = starlark_syntax::dump::psi_to_string(&tree);
```

AGENTS.md, "The consumer contract", says what a consumer may rely on.

Licensed under the Apache License, Version 2.0 (unlike the rest of accio, MIT); see NOTICE for the code it ports.
Its history before the move into accio, from github.com/develar/psi-syntax, is replayed here as a line; that repository is now
this directory's public mirror.
