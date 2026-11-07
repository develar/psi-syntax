# psi-syntax: what an agent must know

Lossless lexers and parsers in Rust, ported from the ones behind IntelliJ's PSI: `psi-syntax/` of the accio repository,
its crates members of accio's Cargo workspace with their own package fields, lints (`[lints]` of each manifest) and
`clippy.toml`; run cargo with `-p <crate>` (decision 2026-11-03-psi-syntax-lives-in-accio in `../docs/decisions/`):

| crate | holds |
|---|---|
| `crates/psi-builder` | the language-neutral half: IntelliJ's `PsiBuilderImpl` (markers, production, edge binders), the preorder column tree and its consumer API, the parse driver that expands chameleons in place, the dump's layout, token-set storage and lexer helpers, all generic over `psi_builder::Language` |
| `crates/kotlin-syntax` | Kotlin (package `kotlin-syntax` for consumers): tokens, lexers, node kinds, binders, the `SemanticWhitespaceAwarePsiBuilder` layer, the parsers, KDoc, and `Kotlin: Language` |
| `crates/java-syntax` | Java (package `java-syntax`): tokens, node types, the Java and JavaDoc lexers (complete), the binders, `Java: Language`, the dumps, and the parsers (see "Java") |
| `crates/csharp-syntax` | C# (package `csharp-syntax`): Roslyn's lexer, directive parser, `LanguageParser` and `DocumentationCommentParser` over a green arena, flattened into `psi_builder`'s tree; `CSharp: TreeLanguage`, the dump, the diagnostics (see "C#") |
| `crates/psi-check` | the tree checks the parsers' tests and `ksdump` share (lossless leaves, links, equal subtrees, nesting and stack limits) and the robustness tests' limits and seeded generators; a dev-dependency of each language |
| `crates/ksdump` | the `ksdump` binary: dumps, benchmarks, lexes, checks bodies (`--lang kotlin\|java\|csharp`, else by extension) |
| `crates/ide-oracle` | the oracle harness, an IDE's own code as the oracle (pinned jars, the IDE's JVM running a single-file oracle, byte-equal comparison naming the first difference, goldens vendored from a revision, Python's seeded generator for mutations), and `java-oracle`, the Java oracle's tooling (`tools/java-oracle/README.md`); accio's `scripts/bench plugin-xml` runs on it too |
| `crates/ab-pairs` | the engine of an A/B of two binaries: interleaved pairs, the first arm alternating, until the stopping rule (`stats::decide`) gives each unit its verdict; the host's measurement reservation and quiet waits; a child timed by its resource usage and instruction counters. Its `ksdump-ab` is the A/B of two ksdump binaries; accio's `scripts/bench` A/B commands run on it |

The goal is exact: for every input, `kotlin_syntax::dump::psi_to_string` prints what Kotlin 2.4.21's
`DebugUtil.psiToString(file, true, false).trim()` prints, byte for byte; `java_syntax`'s dumps print IntelliJ 271's
Java PSI, and `csharp_syntax::dump::tree_to_string` prints Roslyn 5.9.0's syntax tree as `tools/csharp-oracle` dumps
it (nodes, tokens, trivia, diagnostics). Consumers: accio's `crates/extract` (code search; declarations,
call sites, imports; path dependencies) and Branch Deck's `code-symbols` (a revision of github.com/develar/psi-syntax, this directory's public mirror) (diff highlighting, navigation, file summaries). Both read the
tree directly: there is no tree-sitter compatibility layer and there must never be one. `docs/DESIGN.md` describes the
architecture as built, its decisions, the measured numbers and the open items.

## Sources to port (read them, port them faithfully)

Kotlin at 9180ce6a7a7d3627f88b424717583f28ddca052d (2.4.21), a sparse checkout at
`K=/Users/develar/.branch-deck/pool/tree-sitter-kotlin-8608ba7a/1/tree-sitter-kotlin/local/kotlin`. A file absent on
disk is read with `git -C $K show HEAD:<path>`.

| what | source |
|---|---|
| Tokens | `compiler/psi/psi-api/src/org/jetbrains/kotlin/lexer/KtTokens.java`, `KtKeywordToken.java`, `KtSingleValueToken`, `KtModifierKeywordToken` |
| Lexer | `compiler/psi/parser/src/org/jetbrains/kotlin/lexer/Kotlin.flex` (the JFlex spec; `_JetLexer.java` is its generated form), `KotlinLexer.java` |
| Element types | `compiler/psi/psi-api/src/org/jetbrains/kotlin/KtNodeTypes.java` (+ `KtStubElementTypes` names where the dump prints them) |
| Builder | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/SemanticWhitespaceAwarePsiBuilder*.java`, `TruncatedSemanticWhitespaceAwarePsiBuilder.java` (in `kotlin-syntax`), and IntelliJ's `PsiBuilderImpl` (`/Users/develar/.cache/accio-bench/intellij-community/platform/core-impl/src/com/intellij/lang/impl/PsiBuilderImpl.java`, in `psi-builder`): markers, `precede`, `done`, `drop`, `rollbackTo`, `collapse`, `error`, `doneBefore`, `errorBefore`, edge (whitespace/comment) binders, `remapCurrentToken`, lookahead |
| Comment binding | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/KotlinWhitespaceAndCommentsBinders.kt`, plus the platform's default binders (`WhitespacesBinders`) |
| Parser | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/AbstractKotlinParsing.java`, `KotlinParsing.java`, `KotlinExpressionParsing.java`, `KotlinParser.java`, the token-stream patterns (`FirstBefore`, `LastBefore`, `AbstractTokenStreamPattern`, …), `ParseUtils.kt` |
| KDoc | `compiler/psi/parser/src/org/jetbrains/kotlin/kdoc/lexer/KDoc.flex`, `KDocLexer.java`, `kdoc/parser/KDocParser.java`, `KDocLinkParser.kt`, `compiler/psi/psi-api/src/org/jetbrains/kotlin/kdoc/lexer/KDocTokens.java`, `kdoc/parser/KDocElementTypes.java`: the dump expands a doc comment's inner tree |

Keep the Java structure: one Rust function per Java method, the same name in snake_case, the same order, so a diff
against the Java reads line by line. Port behaviour, not just shape: newline sensitivity, rollback, recovery sets,
error messages (the dump prints `PsiErrorElement:<message>`) and the exact element types.

### Java (lexers and parsers ported)

JetBrains/intellij-community at decc943674edfe7ecc620d2d60b61be38f876994: `java/java-syntax` (`_JavaLexer.flex`, `_JavaDocLexer.flex`,
`JavaParser` and its `*Parser.kt`, `JavaSyntaxElementType`, `JavaSyntaxTokenType`, `JavaDocSyntax*`), on the builder of
`platform/syntax/*` (`SyntaxTreeBuilderImpl`, `MarkerProduction`, `WhitespacesBinders`). Read them with
`git -C ~/.cache/psi-syntax/intellij-community show decc943674edfe7ecc620d2d60b61be38f876994:<path>`, never from the moving
working tree. The live oracle is the installed IntelliJ 2027.1 (build 271.SNAPSHOT, built from that revision):
`/Applications/Idea.app/Contents/plugins/java/lib/intellij.java.syntax.jar` with
`/Applications/Idea.app/Contents/lib/intellij.platform.syntax*.jar` and the Kotlin stdlib and util jars beside them.
The same rules hold as for Kotlin: one Rust function per Kotlin/Java method, the same names and order, error messages
verbatim. `java-syntax` implements `psi_builder::Language` as `kotlin-syntax` does.

| what | source (under `java/java-syntax/src/com/intellij/java/syntax/`) | port (`crates/java-syntax/src/`) |
|---|---|---|
| Tokens, element types | `element/JavaSyntaxTokenType.kt`, `JavaDocSyntaxTokenType.kt`, `JavaSyntaxElementType.kt`, `JavaDocSyntaxElementType.kt`, `JShellSyntaxElementType.kt`, `SyntaxElementTypes.kt`, `JavaDocSyntaxTokenTypes.kt` | `tokens.rs`, `node_kind.rs`, `syntax_element_types.rs` |
| Lexers | `lexer/_JavaLexer.flex`, `JavaLexer.kt`, `JavaTypeEscapeLexer.kt`, `_JavaDocLexer.flex`, `JavaDocLexer.kt` (`gen/.../_JavaLexer.kt`, `_JavaDocLexer.kt` for the Unicode tables) | `lexer.rs`, `doc_lexer.rs`, `unicode.rs` (`tools/java-unicode.py`) |
| Binders, lazy elements | `element/WhiteSpaceAndCommentSetHolder.kt`, `JavaWhitespaceOrCommentBindingPolicy.kt`, `element/lazyParser/*.kt`, `JavaSyntaxDefinition.kt`, `JavaDocParser.fakeCollapseRightBinder` | `builder.rs` (`Binder`), `language.rs`, `tree.rs` (`FileKind`, `Entry`, `LazyKind`) |
| Parsers | `parser/*.kt` | `parser/*.rs`, one file per Kotlin file; `JavaParser` wires them; the one unported method, `JavaParserUtil.stoppingBuilder(builder, condition)` (JavaParserUtil.kt:133), has no caller in intellij-community and no stub (`java_parser_util.rs` says why) |
| Messages, levels | `resources/messages/JavaSyntaxBundle.properties`, `pom/java/LanguageLevel.kt`, `JavaFeature.kt` | `bundle.rs`, `language_level.rs` |

### C# (Roslyn's lexer and parser ported)

dotnet/roslyn at 35d9211b841e7613c1d2f8f5af6d628ace696c4c (branch release/insiders, no tag contains it), the commit
NuGet `Microsoft.CodeAnalysis.CSharp` 5.9.0 is built from (its nuspec names it): the lexer and parsers under
`src/Compilers/CSharp/Portable/Parser/`, the kinds, node shapes and green semantics under
`src/Compilers/CSharp/Portable/Syntax/` (`SyntaxKind.cs`, `SyntaxKindFacts.cs`, `Syntax.xml`, `InternalSyntax/`),
the diagnostics under `Errors/` with their messages in `CSharpResources.resx`. Read them with
`git -C ~/.cache/psi-syntax/roslyn show 35d9211b841e7613c1d2f8f5af6d628ace696c4c:<path>` (a blobless clone), never
from a working tree. Scope, what accio needs: full parses of `.cs` files at any `LanguageVersion` (`Latest` by
default, the gate also runs Preview), any `DocumentationMode` (`Parse` by default), any preprocessor symbols (none by
default), diagnostics with their English messages; no scripts (`.csx`), `Features`, incremental parsing, node cache or
body-less parsing (Roslyn reparses no block on its own).

The rules of the Kotlin and Java ports hold: one Rust function per C# method, the same name in snake_case, the same
order, so a diff against the C# reads line by line; error codes and their arguments, recovery, speculation with reset
points and feature checks (`LanguageVersion`, `CheckFeatureAvailability`) exactly as Roslyn has them. C# overloads
take a suffix after the first (`_with_<last parameter>`), `ref`/`out` parameters are `&mut`, a C# local function is a
nested or module-private `fn` right after its method, a call's arguments are evaluated into locals left to right as
C# evaluates them, `ParserSyntaxContextResetter` and disposable reset points are a save and restore where the `using`
scope ends (`parser/mod.rs`, `statements.rs`, `expressions.rs` say it at the top). A green factory is the
`GreenArena` method of `InternalSyntax.SyntaxFactory`'s name (`self.arena().class_declaration(...)`).
`StackGuard.EnsureSufficientExecutionStack` is a depth counter (see the consumer contract, "Nesting", and DESIGN,
"C# nesting and work"); a recursion Roslyn leaves unguarded gets the same counter. Every deviation from Roslyn is
named at its place and listed in DESIGN, "C# deviations".

| source (under `src/Compilers/CSharp/Portable/` unless rooted) | port (`crates/csharp-syntax/src/`) |
|---|---|
| `Syntax/SyntaxKind.cs`, `Syntax/Syntax.xml` | `syntax_kind.rs`: `SyntaxKind` with Roslyn's numbers, `TokenKind` (the 244 `IsAnyToken` kinds and the 8 unstructured trivia), `NodeKind` (the 322 node kinds of Syntax.xml, structured trivia among them), dense, `NODE_BASE` 252; generated |
| `Syntax/Syntax.xml` (`TreeFlattening.FlattenChildren` for its `Choice` and `Sequence` fields) | `syntax.rs`: per node type its kinds, slots in field order and boolean fields, and `InternalSyntax.SyntaxFactory`'s factories as `GreenArena` methods; generated |
| `Errors/ErrorCode.cs`, `Errors/MessageID.cs`, `Errors/XmlParseErrorCode.cs`, `CSharpResources.resx` | `error_code.rs`: the 175 codes `Parser/*.cs` reports and the feature diagnostics, with numbers, severities and English formats, `MessageID` with `RequiredVersion`, `XmlParseErrorCode`; generated |
| `src/Compilers/Core/Portable/Xml/XmlCharType.cs` | `xml_char_type.rs`: its character property tables; generated |
| `CharUnicodeInfo.GetUnicodeCategory` of the oracle's .NET 10.0.x | `unicode.rs`: the lexer's classes of a UTF-16 code unit above ASCII; generated by `tools/csharp-unicode` |
| `Syntax/SyntaxKindFacts.cs` | `facts.rs` (not `GetOperatorKind`, `IsCheckedOperator`: metadata names for the binder) |
| `LanguageVersion.cs`, `CSharpParseOptions.IsFeatureEnabled` | `language_version.rs` |
| `Syntax/InternalSyntax/` (`CSharpSyntaxNode`, `SyntaxToken*`, `SyntaxTrivia`, `SyntaxFirstTokenReplacer`, `SyntaxLastTokenReplacer`), `Errors/SyntaxDiagnosticInfo.cs`, `Errors/XmlSyntaxDiagnosticInfo.cs`, `SyntaxListBuilder`, `SyntaxListPool` | `green.rs`: the green tree as an arena with Roslyn's copy semantics and flags |
| the red tree's positions, `ChildNodesAndTokens()`, `Syntax/SyntaxTreeDiagnosticEnumerator.cs` | `flatten.rs`: green to `psi_builder::Tree<CSharp>` and the diagnostics in Roslyn's order |
| `src/Compilers/Core/Portable/Diagnostic/DiagnosticInfo.cs` (`GetMessage`), `XmlSyntaxDiagnosticInfo` | `diagnostic.rs`: a message as `string.Format` of the English format formats Roslyn's arguments |
| `tools/csharp-oracle/Dump.cs` | `dump.rs`: `tree_to_string`, `lex_to_string`, `index_line` |
| `CSharpSyntaxTree.ParseText`, `SyntaxFactory.Parse*`, `CSharpParseOptions` | `tree.rs` (`Parser`, `Tree`, `FileKind`, `Entry`, `DocumentationMode`, `Diagnostic`), `lexer/mod.rs` (`PreprocessorSymbols`, `LexerOptions`), `language.rs` (`CSharp: TreeLanguage`) |
| `Parser/SyntaxParser.cs`, `Parser/SyntaxParser.ResetPoint.cs` | `syntax_parser.rs`: the token window, lexer modes, reset points, `EatToken`, the error helpers, skipped syntax, keyword conversions, feature checks |
| `Parser/Lexer.cs`, `AbstractLexer.cs`, `Lexer_StringLiteral.cs`, `Lexer_RawStringLiteral.cs`, `Lexer.Interpolation.cs`, `SlidingTextWindow.cs`, `CharacterInfo.cs` (with `UnicodeCharacterUtilities`), `DocumentationCommentXmlTokens.cs`, `QuickScanner.cs`, `LexerCache.cs` (its token map) | `lexer/mod.rs`, `abstract_lexer.rs`, `string_literal.rs`, `raw_string_literal.rs`, `interpolation.rs`, `sliding_text_window.rs`, `character_info.rs`, the XML tokens in `lexer/mod.rs`, `quick_scanner.rs`, `lexer_cache.rs` |
| `Parser/Directives.cs`, `Parser/DirectiveParser.cs` | `lexer/directives.rs`, `lexer/directive_parser.rs` |
| `Parser/LanguageParser.cs` 1-235 and 14420- (state, entry points, `ParseWithStackGuard`, `CreateForGlobalFailure`, `IsTerminator`, reset points, `ParseCommaSeparatedSyntaxList`, `ConsumeUnexpectedTokens`), `SyntaxFactoryContext.cs` (four flags) | `parser/mod.rs` |
| `Parser/LanguageParser.cs` 236-6162: namespaces, usings, attributes, type and member declarations, accessors, parameters, fields, variables, enums, names | `parser/declarations.rs` |
| `Parser/LanguageParser.cs` 6163-8247: type parameters and arguments, `ScanType`, `ParseType`, tuples, function pointers | `parser/types.rs` |
| `Parser/LanguageParser.cs` 8248-11122: statements, blocks, local declarations and functions | `parser/statements.rs` |
| `Parser/LanguageParser.cs` 11123-14419: expressions, lambdas, initializers, queries | `parser/expressions.rs` |
| `Parser/LanguageParser_Patterns.cs` | `parser/patterns.rs` |
| `Parser/LanguageParser_InterpolatedString.cs` | `parser/interpolated_string.rs` (each hole's lexer and parser over a window of the file) |
| `Parser/DocumentationCommentParser.cs` | `parser/doc_comment_parser.rs` (run by the lexer on each documentation comment) |
| `Syntax/Syntax.xml`'s fields, the partial classes `Syntax/*Syntax.cs` (their computed properties: `NameSyntax.Arity`, `GetUnqualifiedName`, `TypeDeclarationSyntax.PrimaryConstructorBaseTypeIfClass`, `ParameterSyntax.IsArgList`, ...) | `syntax_getters/`: `fields.rs` (generated: a module per node type and per abstract class with fields, a getter per field, `is_<abstract class>`, `<field>_of`), `slots.rs` (the slot each child of a node fills, read back from the kinds in Syntax.xml order), a module per class with computed properties (`name.rs`, `type_declaration.rs`, `parameter.rs`, ...), `mod.rs` (`has_modifier`, `slot`, `slot_children`), `getters.rs` (the lines of `oracle.sh getters`) |
| what accio's `extract/src/csharp/nav.rs` and Branch Deck read of a tree beyond the properties, no consumer's facts | `read.rs`: trivia and structured trivia, `span` (`SyntaxNode.Span`), `first_token`, `last_token`, `is_code_token`, `name_leaf`, `word`, `dotted_name`, `name_segments`, `text_without_trivia`, `name_text`, `doc_comment`, `declaration_start`, `type_names`, `type_head` |

Not ported, as they change no tree: `Blender*.cs` and `BlendedNode.cs` (incremental parsing), `LexerCache.cs`'s trivia
map and keyword cache (keywords are a perfect hash; its token map is `lexer/lexer_cache.rs`, which `QuickScanner.cs`,
`lexer/quick_scanner.rs`, reads), the node cache. The
generated files are never edited by hand: `tools/csharp-gen/gen.py [--roslyn DIR] [--rev REV]` writes
`syntax_kind.rs`, `syntax.rs`, `error_code.rs`, `xml_char_type.rs`, `syntax_getters/fields.rs` and the oracle's
`tools/csharp-oracle/Fields.g.cs` from `git show`; `DOTNET_ROOT=$HOME/.cache/psi-syntax/dotnet
$HOME/.cache/psi-syntax/dotnet/dotnet run --project tools/csharp-unicode > crates/csharp-syntax/src/unicode.rs` writes
`unicode.rs`. Tests (`crates/csharp-syntax/tests/`): `goldens.rs` (the 7064 goldens of `testdata/csharp` and the
reduced cases of `tests/oracle/`, `NAME.cs` with the oracle's `NAME.tree.txt` and a `manifest.tsv` line, strict under
`KS_STRICT=1`; `CSHARP_TESTDATA` names another directory), `robustness.rs` (see DESIGN, "C# nesting and work"),
`kinds.rs` (the generated tables against each other and `facts`), `flatten.rs` (hand-built green trees dumped),
`pipeline.rs` (files, entries and preprocessor symbols end to end), `memory.rs` (the arena budget, the retained
buffers), `value_text.rs` (`Tree::value_text` against Roslyn's lexer tests, and `tests/values/` and the goldens against
the oracle's `values --kept` lines), `directives.rs` (`Tree::directives` of the goldens against the oracle's
`directives` lines in `tests/directives/`), `contract.rs` (the consumer contract's contextual keywords, converted or not),
`syntax_getters.rs` (the getters of declarations, names, usings and expressions, each Roslyn's answer; every getter on
every node of the goldens, of 3000 random inputs and of deep and long ones, 30 000 members or terms and 1000 levels,
on a 256 KiB stack and in linear time; every child of every node in a slot; the `<field>_of` dispatchers equal to
their fields; `read::span` equal to the one-pass spans; a getter reads only up to its field).

The oracle is that package on .NET SDK 10.0.403 (`~/.cache/psi-syntax/dotnet`, pinned by `global.json`, packages by
`packages.lock.json`), run with `DOTNET_ROOT=~/.cache/psi-syntax/dotnet DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1`
(`oracle.sh` sets the first): `tools/csharp-oracle/oracle.sh dump --list LIST --root ROOT --out OUT [--version
latest|preview|N] [--doc parse|none|diagnose] [--define A;B] [--entry NAME] [--stack-mb M]` writes `OUT/<path>.tree.txt`
and `index.jsonl`; `oracle.sh lex --list LIST --root ROOT [--doc ...]` prints the lexer hash lines; `oracle.sh values
--list LIST --root ROOT [--kept]` the tokens whose `ValueText` is not their text (`--kept`: of those the port keeps the
value of, as `ksdump --values` prints ours); `oracle.sh directives --list LIST --root ROOT` each directive's `IsActive`,
`BranchTaken` and `ConditionValue` (as `ksdump --directives` prints ours); `oracle.sh getters --list LIST --root ROOT [--full OUT]` the hash line of
each file's syntax getter lines (Roslyn's properties of every node, `--full`: the lines, `OUT/<path>.getters.txt`). Ours: `target/release/ksdump --lang csharp --list LIST --root ROOT --out
OUT [--version V] [--doc D] [--define A;B]` (a list line `PATH[<TAB>VERSION[<TAB>ENTRY[<TAB>DOC[<TAB>SYMBOLS]]]]`,
SYMBOLS the preprocessor symbols separated by `;`, the flags filling empty columns), `--lex` for the lexer lines,
`--check ORACLE`, `--lex --check LINES`, `--values --check LINES`, `--directives --check LINES` and `--getters --check LINES` for the gate's in-memory comparisons, `--bench` for
throughput, `--check-bodies` for losslessness alone; `scripts/compare.py ORACLE_DIR OURS_DIR` buckets differing
`.tree.txt` dumps by their first differing line. `tools/csharp-oracle/README.md` is the normative dump format, the
goldens' extraction and the caches: `testdata/csharp/` (7064 goldens of Roslyn's parser and lexer tests, `manifest.tsv`
with version, entry, documentation mode and preprocessor symbols), the corpora dotnet/roslyn at the pin, dotnet/runtime
and dotnet/aspnetcore v10.0.14, AvaloniaUI/Avalonia 12.1.4 under `~/.cache/psi-syntax/csharp-src/` with lists in
`corpus/csharp-*.list`, dumps in `oracle/csharp-*` (and `-preview`), lexer lines in `lex/csharp-*`, mutations and lexer
fuzz in `fuzz/csharp-*` (with `-latest` and `-preview` dumps), and of the roslyn and Avalonia files whose conditions
name a symbol, with all of them defined, `corpus/csharp-*-defines.list`, `oracle/csharp-*-defines` and
`lex/csharp-*-defines.doc.oracle.txt` (`tools/csharp-oracle/defines.py`), directive lines in `directives/`;
`tools/csharp-oracle/caches.sh` rebuilds them,
`vendor-goldens.sh` the goldens. A difference found on a corpus is reduced to a small input, which goes to
`crates/csharp-syntax/tests/oracle/` with the oracle's dump once fixed. The gate's C# steps are 9 (the lexer at `--doc
none`, `parse` and `diagnose` against the cached lines), 10 (the dumps of every corpus at Latest and Preview, the
mutations at their goldens' versions, Latest and Preview, the fuzz at Latest and Preview; the 6 runtime files without an
oracle dump only lossless), 11 (the dumps and lexer lines of the `-defines` lists, every symbol of a file's
conditions defined) 12 (the directives' flags of the corpora, the `-defines` lists, the mutations and the fuzz) and 13 (the syntax getters against the cached `getters` lines of the goldens, the corpora at Latest
and Preview, the roslyn corpus at the other documentation modes, the `-defines` lists, the mutations, the lexer fuzz
and the getters fuzz of `tools/csharp-oracle/getters_fuzz.py`); see `scripts/gate.sh`'s header. While iterating: `cargo nextest run --release -p csharp-syntax
--test goldens --no-capture` (the pass count and the first differing lines), then one corpus with `ksdump --check`.

Updating Roslyn (a new `Microsoft.CodeAnalysis.CSharp` release):

1. Find the new commit in the package's nuspec (`<repository commit=...>`), fetch it into the blobless clone
   (`git -C ~/.cache/psi-syntax/roslyn fetch origin <commit>`), and record the tag that contains it, if any.
2. Read what changed between the two commits:
   `git -C ~/.cache/psi-syntax/roslyn diff OLD NEW -- src/Compilers/CSharp/Portable/Parser/ src/Compilers/CSharp/Portable/Syntax/SyntaxKind*.cs src/Compilers/CSharp/Portable/Syntax/Syntax.xml`,
   and the same for the other sources of the table (`Syntax/InternalSyntax/`, `Errors/`, `LanguageVersion.cs`,
   `CSharpParseOptions.cs`, `CSharpResources.resx`) and for the tests the goldens come from
   (`src/Compilers/CSharp/Test/Syntax/Parsing/`, `LexicalAndXml/`).
3. Move the pin everywhere it is written (`git grep -e <old commit> -e <old version>`, outside `testdata/`): this
   section, NOTICE, `tools/csharp-gen/gen.py` (`PIN`), `tools/csharp-oracle/CSharpOracle.csproj` (the exact version;
   then restore once with `CSHARP_ORACLE_UNLOCK=1` to rewrite `packages.lock.json`), `vendor-goldens.sh`, `caches.sh`
   (the roslyn corpus), the oracle's README, `lexer/directive_parser.rs` (`PRODUCT_VERSION`, what `#error version`
   prints: take it from the new oracle's dump of `#error version`), the crate's doc comments that name the version, and
   `global.json` with `tools/csharp-unicode` when the package needs a newer SDK.
4. Rerun the generators (`gen.py`; `tools/csharp-unicode` when the .NET runtime changed) and `cargo nextest run
   --release -p csharp-syntax --test kinds`; a new kind, node type or error code shows up in the generated files' diff.
5. Port the diff method by method into the files of the table, in C# order, with new deviations named at their place
   and in DESIGN.
6. Regenerate the oracle's answers: `tools/csharp-oracle/vendor-goldens.sh` (the goldens; read its printed counts
   against the README's), then `tools/csharp-oracle/caches.sh` (a roslyn corpus at the new pin gets a new list and
   key, so `scripts/gate.sh`'s corpus names move with it; the mutations and fuzz are regenerated from the new goldens).
   Never edit a dump to make a test pass.
7. `cargo nextest run --release --profile heavy -p csharp-syntax`, then `scripts/gate.sh` PASS; record the new counts
   and any moved measurement in DESIGN with the commit and the date.

## Layout

Paths of `kotlin-syntax` are under `crates/kotlin-syntax/`, of `psi-builder` under `crates/psi-builder/`.

| module | holds |
|---|---|
| `psi-builder` `src/language.rs` | `TreeLanguage` (what a tree needs: token and node types, the file kind, the error element), `Language: TreeLanguage` (what the builder and driver ask a language besides: binders, whitespace and comments, left-bound types, chameleons and bodies, the cursor's joined token, brackets, how a file and a chameleon are lexed and parsed), `TokenType` (with `NODE_BASE`, where node indices start), `NodeType`, the generic `ElementType<T, N>` |
| `psi-builder` `src/builder.rs` | the PsiBuilder port: token cursor that skips whitespace and comments, markers with rollback (the production a linked list, so `precede` and the other middle edits take constant time), the `WhitespacesAndCommentsBinder` contract, edge balancing, the nesting limits (a language's own through `Language` constants), the balanced-block and too-deep skips, block ends in amortized constant time (`balanced_block_end`); binds the tree |
| `psi-builder` `src/tree.rs` | the lossless tree: `u32` ids in preorder over three columns (`kinds` u16: element type index plus leaf, unexpanded, trailing and missing bits; `ranges` [start, end]; `links` [parent, subtree_end]), 18 bytes a node; a node's subtree is `id..subtree_end`, children in order, tokens as leaves; whitespace and comments are leaves where PSI puts them. `Parser<L>` drives the builders: the file's, then a stack of chameleon builders, each expanding its chameleon in place where `bind` meets it (heap, not recursion), through `Language::parse_chameleon` and its `Chameleon` (the body's file tokens when reusable, else relexed). `TreeBuilder<L>` builds a tree in preorder for a language whose parser does not run on `PsiBuilderImpl`, writing its columns in place (pushed, or by id through `ColumnWriter` into columns sized up front) and handing them to the tree. The consumer API: see "The consumer contract" |
| `psi-builder` `src/stack.rs` | the stack a parse may take: `STACK` (1 MB, the promise), `STACK_BUDGET` (three quarters of it, for the levels), `StackGuard` (where a builder's parse started; `used`), `StackUse` (the deepest level and the most stack of a parse, `Parser::stack_use` of each language) |
| `psi-builder` `src/buffers.rs` | what a reusable parser keeps of its buffers: `RETAINED_BYTES` (64 MiB), `retained_after` (half), `Shrink` (every buffer emptied to the same share of its capacity), shared by the three languages' `Parser`s |
| `psi-builder` `src/dump.rs`, `src/token_set.rs`, `src/lexer.rs`, `src/counters.rs` | the dump's layout (`psi_to_string_with`, a language labels each node; `fix_white_spaces`); `ElementTypeSet`, a `TokenSet`'s bits; `Lexed<T>` (types, starts, plain bits), `char_at`, `in_ranges`; the builder's and driver's event counters |
| `psi-builder` `src/document.rs` | a file's text as IntelliJ's document holds it: the byte order mark (`has_byte_order_mark`, `blank_byte_order_mark`), `Lines` (the line of an offset); each language's `document_text` (`tree.rs` of `kotlin-syntax`, `java-syntax`) builds on it |
| `src/tokens.rs` | `TokenKind` (every Kotlin and KDoc token, plus WHITE_SPACE, BAD_CHARACTER), its debug name as the dump prints it, the keyword sets (hard, soft, modifier) |
| `src/unicode.rs` | `[:letter:]`, `[:digit:]` (Kotlin.flex) and `[:jletter:]` (KDoc.flex) above ASCII, decoded from the character maps of `_JetLexer.java` / `_KDocLexer.java`; generated, do not edit |
| `src/lexer.rs`, `src/kdoc/lexer.rs` | the ports of Kotlin.flex and KDoc.flex (states, string templates, raw strings, `$$` prefixes, nested block comments, shebang) |
| `src/kdoc/parser.rs` | `KDocParser` and `KDocLinkParser`: the tree runs them on every doc comment and markdown link |
| `src/node_kind.rs` | `NodeKind`, the composite element types with their dump names (`FUN`, `CLASS`, `KtFile` root, `PsiErrorElement`) |
| `src/language.rs` | `Kotlin`, the `Language` impl: `KotlinParserDefinition`'s whitespace and comments and file type, the lazy element types (BLOCK, LAMBDA_EXPRESSION, KDoc, markdown link) and how each is lexed and parsed |
| `src/builder.rs` | Kotlin's binders (`Binder`: the platform defaults, `PrecedingCommentsBinder`, `KotlinWhitespaceAndCommentsBinders`, ...), `SemanticWhitespaceAwarePsiBuilderImpl` (newline mode and join stacks, `getJoinedTokenType`) and its adapters seen through the `SemanticWhitespaceAwarePsiBuilder` handle; re-exports the builder for Kotlin's types (`PsiBuilderImpl`, `Marker`, `Nesting`, the limits) |
| `src/tree.rs` | `Tree`, `Parser` (`psi_builder`'s over `Kotlin`), `FileKind`, `LazyKind` |
| `src/parsing/*.rs` | `abstract_parsing.rs`, `kotlin_parsing.rs`, `expression_parsing.rs`, `patterns.rs`: the parser ports; `kotlin_parser.rs`: the `KotlinParser` entry points |
| `src/dump.rs` | `psi_to_string(&Tree, file_name) -> String`: the exact PSI debug format (Kotlin's labels over `psi_builder::dump`) |
| `src/psi.rs` | `kotlin_syntax::psi`: the getters of Kotlin's PSI classes (`compiler/psi/psi-api/.../psi/`) as free functions over `&Tree` and `NodeId`, one per Java or Kotlin method, its source in the doc comment: modifiers and visibility (`KtModifierListOwner`, `KtModifierList`, `ktPsiUtil.kt`), annotation entries, use-site targets and short names, names and name identifiers, classes and objects, super type entries, receivers and declared types, parameters, type parameters and constraints, properties and accessors, destructuring entries, type references through nullable and parenthesized wrappers, user types, function types, simple names, imports, qualified and call expressions, value and lambda arguments |
| `src/counters.rs` | the parser's own event counters (semantic layer, chameleons, KDoc tokens, soft keywords) and `ALL`, every counter of a parse in the order `ksdump` prints them |
| `java-syntax` `src/lexer.rs`, `src/doc_lexer.rs`, `src/unicode.rs` | `JavaLexer` (by hand: whitespace, comments, literals, text blocks, string templates, unicode escapes) over `FlexJavaLexer` (the rules of `_JavaLexer.flex`), `JavaTypeEscapeLexer`; `JavaDocLexer` (merging adapter, `AsteriskStripperLexer`) over `FlexJavaDocLexer` (every state of `_JavaDocLexer.flex`, longest match, earlier rule on a tie); `[:jletter:]`, `[:jletterdigit:]` above ASCII, generated |
| `java-syntax` `src/builder.rs` | `SyntaxTreeBuilder` (the builder as a Java parser method receives it, with `JavaParserUtil.stoppingBuilder`'s stop offset), the `SyntaxBuilderUtil` helpers (`advance`, `expect`, `expect_set`, `drop`, `raw_token_text`, `parse_block_lazy`), Java's `Binder`s and `WhiteSpaceAndCommentSetHolder` |
| `java-syntax` `src/language.rs`, `src/tree.rs` | `Java`, the `Language` impl (whitespace and comments, left-bound types, the five lazy element types and their lexers and parsers, `parse_fragment`); `Tree`, `Parser`, `FileKind { level, entry }` (`Entry::File` or one of the 13 parser-partial entries), `LazyKind` |
| `java-syntax` `src/parser/*.rs` | `java_parser.rs` (`JavaParser`, `JavaSyntaxDefinition.parse`), `java_parser_util.rs` and `java_keywords.rs` (ported), `file_parser.rs`, `module_parser.rs`, `declaration_parser.rs`, `statement_parser.rs`, `expression_parser.rs` (ported: delegates), `pratt_expression_parser.rs`, `reference_parser.rs`, `pattern_parser.rs`, `java_doc_parser.rs`, `jshell_parser.rs`: every method a function, all ported but `JavaParserUtil.stoppingBuilder(builder, condition)`, which nothing calls (`java_parser_util.rs` says why) |
| `java-syntax` `src/psi/` | `java_syntax::psi`: the getters of IntelliJ's Java PSI classes over the tree, a module per class of `java/java-psi-impl` at the parser's revision (`field` is `PsiFieldImpl`, `class` is `PsiClassImpl` with `ClassElement`'s roles, `code_reference` is `PsiJavaCodeReferenceElementImpl`, `modifier_list` is `PsiModifierListImpl`, ...), a function per method in snake_case with its source line; `mod.rs` holds the interface methods dispatched on the element type (`get_modifier_list`, `has_modifier_property`, `get_name_identifier`, `get_type_element`, `get_initializer`, `get_containing_class`); `getters.rs` prints every getter of every node as `tools/java-oracle/JavaPsiOracle.java --getters` prints the real PSI's |
| `java-syntax` `src/dump.rs` | `psi_to_string` (`DebugUtil.psiToString`: PSI class names and their `toString()`, the labels the goldens show) and `node_tree_to_string` (`DebugUtil.nodeTreeAsElementTypeToString`, the `_node.txt` goldens) |
| `java-syntax` `tests/lexer.rs`, `tests/goldens.rs`, `tests/doc_comments.rs`, `tests/robustness.rs` | the leaf texts of all 1036 Java goldens against our tokens (adjacent tokens joined where the parser collapses them, reference and type holders relexed); every golden parsed at its level through its entry, both dumps against `NAME.txt` and `NAME_node.txt`; `JavaDocParser` alone, each golden parsed flat and its `PsiDocComment` subtrees compared with the golden's (`JAVA_STRICT=1` or `KS_STRICT=1` fails on any difference); 80 recursive forms nested 100 000 deep, 44 chains and lists of 100 000 and 40 random mixes on a 1 MB stack, random bytes and fragments, every golden under nesting limits 1 to 13 (lossless, `parse_body` exact) |
| `java-syntax` `tests/psi.rs` | the getters on the inputs where a consumer once read the tree differently from IntelliJ (the answers the live PSI's), every getter on every node of the goldens, of 3000 random inputs and of deep and long ones (a declaration of 30 000 fields, 1000 array levels, an initializer 30 000 deep), `field::declaration_heads` against the per-field walk |
| `csharp-syntax` `src/`, `tests/` | every module with the Roslyn file it ports, and every test: "C#" above |
| `crates/ksdump/src/main.rs`, `check.rs`, `pool.rs` | `ksdump --list LIST --root ROOT --out OUT`: writes `OUT/<path>.psi.txt` per file like the oracle, and `ksdump --bench --list LIST --root ROOT [--threads N] [--walk] [--no-bodies] [--repeat K] [--file-stats OUT.tsv] [--retain MIB|max]`: parse throughput in MB/s (one thread or N), the cost of a preorder walk and of a linear scan, tree bytes per source MB, body-less parsing, the parser's buffers at the end of each parse and after the round (`--retain` sets the retained bytes; the TSV's `parser_grown` and `parser_kept` per file); `--features alloc-stats` adds allocations per file, `--features counters` the builder's calls per token. `ksdump --lex --bench [--docs]` times the file lexer alone (Kotlin's or Java's, a list of one language), with `--docs` also the doc-comment lexer over every doc comment; `--lex` hashes the tokens `tokenize_into` makes, as a parse lexes them; `ksdump --check-bodies` checks that the leaves are the text and `parse_body` against the full parse; `--nesting-limit N` parses with that limit (the too-deep paths on ordinary files); `--lang kotlin|java|csharp` (else `.java` is Java, `.cs` C#, anything else Kotlin); `--lex` lexes Java files (a list line may be `PATH<TAB>LEVEL`), `--out` dumps them (a list line as `JavaPsiOracle.java` reads it, `--node` adds `.node.txt`), `--bench` takes a list of Java files (`PATH[<TAB>LEVEL]`, never mixed with Kotlin), `--check-bodies` takes one as `--out` does, `--getters` writes (`--out`) or compares (`--check`) the lines of `java_syntax::psi::getters` instead of the dumps; C# (`--lang csharp` or `.cs`): `--out` and `--lex` take list lines as `tools/csharp-oracle` reads them, `--version V`, `--doc parse|none|diagnose` and `--define A;B` fill their empty columns (`--bench` parses without the symbols), `--bench` takes a list of C# files (parse and flatten), `--lex --bench` times the lexer alone, `--values` prints the value text lines as `tools/csharp-oracle values --kept` does (`--values --check LINES` compares them with its cached ones, the gate's), `--directives` the directive lines as `tools/csharp-oracle directives` does (`--directives --check LINES` the same), `--getters` the hash lines of `csharp_syntax::syntax_getters::getters` as `tools/csharp-oracle getters` does (`--full OUT` or `--out OUT` writes the lines, `--getters --check LINES` compares them with its cached ones), `--check-bodies` only checks that the leaves are the text (C# has no lazily parsed bodies). `ksdump --check ORACLE` makes the dumps `--out` would write in memory and compares them byte for byte with the oracle's directory (a dump that differs or that one side lacks; `index.jsonl` aside), writing only the differing ones to `--out`; `--lex --check LINES` compares the `--lex` lines with the oracle's cached ones; both print `N differing of M` and exit nonzero on any (the gate's comparisons). `--out`, `--check`, `--lex` and `--check-bodies` parse on every core (`--threads N`), one `Parser` per thread, the files largest first, the output in list order. Features `counters` (both crates' per-thread counters; without it every `add` compiles to nothing) and `alloc-stats` |
| `tests/goldens.rs` | every pair under `testdata/{psi,spec}`: parse the `.kt`/`.kts`, compare `psi_to_string` with the `.txt` (trailing whitespace trimmed); prints the pass count and the first differing line of each failure; `KS_STRICT=1` fails on any difference but the stale pair of `tests/common::KNOWN_STALE` |
| `tests/oracle.rs` | each `tests/oracle/NAME.kt` against the live PSI oracle's dump `NAME.txt` of it: inputs that once differed |
| `tests/lexer.rs`, `tests/kdoc.rs`, `tests/robustness.rs` | the goldens' leaf texts against our tokens; the goldens' `KDoc` subtrees against ours; arbitrary bytes parse losslessly without a panic, ids in preorder, navigation consistent with the parent links; 22 recursive forms nested 100 000 deep and 18 left-associative chains of 100 000 operators on a 1 MB stack, each well under a second; every golden under nesting limits 1 to 13, lossless and with `parse_body` exact |
| `tests/psi_getters.rs`, `tests/common/getters.rs`, `examples/psi_getters.rs` | `kotlin_syntax::psi` against the live PSI's getters: the cases of `tests/psi/` (`NAME.kt` and the oracle's `NAME.kt.getters.txt`), every golden against the oracle's hashes in `tests/psi/goldens.hash`, and every getter on every node of random bytes, fragments and cut goldens without a panic; the printer of the oracle's lines, which the example writes for a list (`--out`) or hashes (`--hash`) |
| `tests/bodies.rs` | body-less parsing on every golden: the full tree minus each body's interior, `parse_body` equal to the full subtree; `leaves()` and `descendants()` |

## The consumer contract

What accio and Branch Deck may rely on (`psi-builder` `src/tree.rs`, named for Kotlin by `kotlin_syntax::{Tree, Parser}`):

- One `Parser` per thread (`Parser::new()`, then `parser.parse(text, FileKind)` per file): it keeps every buffer of a
  parse and reuses it; `Tree::parse` makes a fresh one per call. `Parser` is `Send`; `Tree` is `Send + Sync`, owns
  its copy of the text (`Arc<[u8]>`) and has no lifetime, so a tree can be handed to other threads (a const
  assertion in `tree.rs` keeps this true).
- Retained buffers, the same for the three languages' `Parser`s: a parse that leaves more than the retained bytes in
  the buffers (`psi_builder::buffers::RETAINED_BYTES`, 64 MiB, or `parser.set_retained_bytes(bytes)`; `usize::MAX`
  keeps all) shrinks them to half of that, each buffer to the same share of its capacity, so a long-lived parser holds
  at most that between files whatever it parsed (a C# parser held 1.24 GB after dotnet/runtime's `hugeexpr1.cs`, now
  32 MiB). `parser.heap_bytes()` is what the buffers hold, `parser.shrink_to(bytes)` shrinks them on demand (`0`
  frees them); trees are unaffected. A file that needs more grows them again: 7 ms for each of the runtime files
  that pass the bound (9 of them; 14 while the tree builder kept a buffer), nothing for the files after them
  (DESIGN, "Retained buffers").
- Ids are preorder: `0..len()` is document order, the subtree of `id` is `id..subtree_end(id)`. `root`, `kind`,
  `token_kind`, `node_kind`, `is_leaf`, `range`, `text`, `parent`, `first_child`, `last_child`, `next_sibling`,
  `children`, `first_child_of_kind`, `preorder` (with depth), `tokens`, `error_message`; `descendants(id)` is the
  id range, so "every `CALL_EXPRESSION` under X" is a scan of one column.
- What both consumers read the tree with, the same for every language: `previous_sibling` (a climb from the id
  before, never a scan of the siblings), `ancestor_or_self(id, accept)`, `deepest_covering(range)` (the last node in
  preorder that holds it), `descendants_pruned(id, prune)` (a declaration's own nodes, a type's names without its
  arguments), `utf8_text`, `errors()` (the error elements in document order with their messages), and
  `leaf_index().at(offset)`, a binary search that relies on the leaves being in offset order and tiling the text
  but for unexpanded bodies (`tests/robustness.rs` checks it on every tree). What a consumer makes of them
  (symbols, call sites, scopes, the text of an error) stays in the consumer.
- The answers only the language has, which a consumer must not keep a list of token kinds for (`TreeLanguage`, each
  implementation delegating to the platform's own answer; `tests/reading.rs` of each language): `is_trivia` (the
  parser definition's whitespace and comments, Roslyn's `SyntaxFacts.IsTrivia` and its structured trivia),
  `is_doc_comment` and `in_doc_comment` (`PsiDocComment`, KDoc, C#'s documentation trivia parsed or left a comment:
  what a doc comment holds is of the comment, not of the code), `composite_children` and `significant_children`, and
  `declaration_range(id)` with `declaration_end(id)`: a declaration without the comments its binders put inside it
  at its edges and without a trailing token that separates it from the next element rather than naming it
  (`TreeLanguage::is_separator_of`: Kotlin's enum entry holds its `,`, while a field's and a method's `;` is their
  own text in all three languages, and Java's and C#'s enum separators are the list's). `own_end` is the end alone,
  without the descent to the first token, which a left-deep chain makes as long as the chain. Not the builder's
  `Language::is_whitespace_or_comment`, which is what a parse skips between tokens (Java's
  `DOC_COMMENT_LEADING_ASTERISKS` with it).
- What a declaration is, as PSI reads it: `kotlin_syntax::psi` (`src/psi.rs`), the getters of Kotlin's PSI classes
  ported method by method, so a consumer asks the compiler's question instead of walking `NodeKind`s: `has_modifier`,
  `visibility_modifier_type` (the set's order, `private internal` is private), `get_annotation_entries` (with the
  entries of `@[A B]`), `kt_annotation_entry_get_short_name` (backticks removed) and its use-site target,
  `get_name_identifier` and `get_name` (unquoted; `Companion`, constructors, `<anonymous>`), the class and object
  tests, `kt_class_or_object_get_super_type_list_entries` (an enum entry's initializer list),
  `get_receiver_type_reference`, `get_type_reference`, `kt_type_reference_get_type_element`, `unwrap_nullability`
  (`Foo??`, `(@A Foo)?`), `kt_user_type_*`, `get_referenced_name` (of every simple name: a label's `foo`, an
  operation reference's `+=` or infix `to`), `kt_property_is_local`, `get_initializer` (an error element is none; an
  explicit backing field's too), `kt_destructuring_declaration_get_entries` (inside the error element of a top-level
  one), the parameter, import, qualified, call and argument getters. A method shared by several classes (an
  interface's, `KtCallableDeclaration`'s, an extension of `ktPsiUtil.kt`) has the bare name and dispatches on the
  node's kind as the virtual call does; a method of one class carries the class (`kt_class_is_interface`) and checks
  the node's kind, as Java's types do; a node whose class lacks the method answers `None` (the dangling modifier list
  of `class C { private }` has no owner, so `has_modifier` of the class body is false). They answer PSI's tree branch (no stubs), `None` where the Java throws,
  iterators where it returns a list, and allocate nothing but `kt_import_directive_get_imported_fq_name`.
  `tools/kotlin-psi-getters/GetterOracle.java` prints Kotlin 2.4.21's own answers for every element (`--out`,
  `--hash`, `--classes` for the PSI classes of each element type); on 18211728 plus the review's fixes (2026-11-03)
  the port printed the same lines for the 789 goldens (480 601 lines), the 55 501 community files (126 353 205
  lines), the 12 000 parse and lexer fuzz inputs and the 16 724 golden mutations (23.8M lines). What a consumer
  makes of the answers (symbols, scopes, a JVM name) stays in the consumer.
- The text to parse is the file as IntelliJ's document holds it, at the file's offsets: `kotlin_syntax::document_text`
  (a byte order mark as spaces, a `\r` as the line end the compiler reads) and `java_syntax::document_text` (the mark
  only: the Java lexer reads every line terminator), over `psi_builder::document`, which also has `Lines`, the
  1-based line of an offset by its `\n`s. The parsers read the bytes they are given: a file parsed as it is has a
  mark as a bad character and, in Kotlin, a `\r` as no line end.
- Java PSI getters (`java_syntax::psi`): what a consumer asks of a declaration, a type or a reference, IntelliJ's own
  answer, so neither reimplements it on raw kinds: a field's or local's type element and modifier list across
  `int a, b[];` (`field::get_type_element`, `local_variable::get_modifier_list`; `field::declaration_heads` and the
  `_with_head` getters for every field of a class in one pass), `has_modifier_property` with the implicit modifiers
  (an interface method `public abstract` unless `private`, an enum `final` or `sealed`, a multi-catch parameter
  `final`, ...; no `PsiAugmentProvider`), `has_explicit_modifier`, the class keyword and kind (`class::is_interface`,
  `is_annotation_type`, ...), the reference lists, the name identifier, a type element's shape and dimensions
  (`type_element::calculate_type`, without resolving `var`), the C-style brackets after a name or a parameter list
  (`java_shared_impl_util::collect_annotations`), a reference's name, qualifier, type arguments and `get_kind_enum`,
  a call's callee and arguments, `new`'s parts and `is_array_creation`, imports, the package name
  (`java_source_util::get_reference_text`, the tree-pure part of `getQualifiedName`). Free functions over `&Tree` and
  the element's `NodeId`, named after the method, `None` where PSI returns `null`, a light element or throws; no
  allocation but the texts, no panic, no recursion. Each is checked against the real PSI on every node of the gate's
  Java inputs (`ksdump --getters --check`, 81.9 million lines, 0 differing). What a consumer makes of the answers
  (its symbols, scopes, spellings, a rule it keeps on purpose) stays in the consumer.
- Highlighting reads `leaves()`: each token leaf's `(TokenKind, range)` in document order. Not the lexer: soft keywords
  are remapped by the parser (`IDENTIFIER` becomes `by`, `get`, `data`, ...) and `?.`, `?:`, `!!` are joined by the
  parser from two lexer tokens, so only the tree's leaves are exact.
- Body-less parsing: `parser.parse_with_options(text, kind, ParseOptions { expand_bodies: false })` leaves every lazily
  parsed block and lambda (what PSI builds as a chameleon) a childless node with its kind and range,
  `is_unexpanded(id)`; doc comments are still expanded. `tree.parse_body(id)` (or `parser.parse_body(&tree, id,
  options)`) parses one on demand into its own `Tree` over the same source, exactly the full parse's subtree there
  (the gate checks all 229 960 bodies of the community files). For declaration-only passes, summaries, navigation;
  a pass over call sites parses in full. Measured on the community files, one thread, 72c8c11: Kotlin
  111.3 -> 231.8 MB/s, nodes 63.7M -> 31.2M, tree 7.64 -> 4.26 bytes per source byte; Java 120.0 -> 265.2 MB/s,
  nodes 85.3M -> 38.6M, tree 8.12 -> 4.24.
- C# (`csharp_syntax::{Parser, Tree, FileKind, PreprocessorSymbols}`, `src/tree.rs`): the same `Parser` per thread
  (`Parser::new()`, then `parser.parse(text, FileKind::default())`, or `FileKind::file(LanguageVersion::CSharp12)`,
  with `doc_mode` for `DocumentationMode::None` or `Diagnose`; `entry` is for Roslyn's tests and hidden).
  Preprocessor symbols are `parser.parse_with_symbols(text, kind, &symbols)` with `PreprocessorSymbols::new(["DEBUG",
  "NET8_0"])`, `CSharpParseOptions.WithPreprocessorSymbols`: an `#if` reads a name the file neither `#define`s nor
  `#undef`s before it as defined when `symbols` holds it (its value: `A\u0042` is `AB`); `parse` defines none, as
  `CSharpParseOptions.Default`. The tree is Roslyn's for those symbols: the regions they enable are code, the others
  `DisabledTextTrivia`. `Parser` is `Send`, `Tree` is
  `Send + Sync` with its own `Arc<[u8]>` (the const assertion is in `tree.rs`), and derefs to `psi_builder::Tree<CSharp>`,
  so every method above applies, with `TokenKind` and `NodeKind` of Roslyn's names (`IdentifierToken`,
  `ClassDeclaration`). The tree is Roslyn's red tree: a node's range is its full span (with the trivia of its
  tokens); a token is a leaf over its span, between its leading trivia's leaves and its trailing trivia's, all
  children of the token's parent; `is_trailing(id)` says a trivia leaf, or the root of a structured trivia, belongs to
  the token before it, otherwise it leads the token after it; `is_missing(id)` marks an empty token the parser
  inserted. Unstructured trivia (whitespace, end of line, comments, disabled text, ...) are leaves of `TokenKind`s;
  structured trivia (directives, documentation comments in `Parse` and `Diagnose` modes, skipped tokens) are subtrees
  whose root is the trivia's `NodeKind`. `leaves()` is exact for highlighting: contextual keywords the parser converts
  (`partial`, `async`, `await`, `record`, `where`, accessors, query clauses, `global::`, pattern combinators, `with`,
  `when`, `file`, `required`, `scoped`, `allows`, ...) carry their keyword kind, the ones it does not stay
  `IdentifierToken` (`var` as a type, `nameof(x)`, any contextual keyword used as a name; `tests/contract.rs`, each
  case Roslyn's tree), `>>` and `>>>` are the parser's merged tokens.
  `tree.directives()` lists every directive's node (`#if`, `#region`, …, a branch not taken's too) with Roslyn's
  `IsActive`, `BranchTaken` and `ConditionValue` for the tree's symbols (`DirectiveFlags`; `directive_flags(id)` of
  one), what tells an inactive directive from an active one without a lex; exact to Roslyn on every gate input
  (`ksdump --directives --check` against `oracle.sh directives`; the goldens in `tests/directives.rs`).
  `tree.value_text(id)` is `SyntaxToken.ValueText` of a token leaf, exact to Roslyn on every gate input (`ksdump
  --values --check` against `oracle.sh values --kept`; the goldens in `tests/value_text.rs`): an identifier without
  its verbatim `@` and with its unicode escapes (`\uXXXX`, `\UXXXXXXXX`) decoded and formatting characters dropped
  (`@class` is `class`, `a\u0062c` is `abc`; in a documentation comment's name attribute the `@` stays, as in Roslyn:
  `<param name="@a"/>` is `@a`), an XML entity's character, a cref's `{` `}` as `<` `>`, a directive string's content;
  the text for keywords, punctuation and the rest, and the empty text of a missing token of any kind; `None` for
  nodes, trivia, text that is not UTF-8 and the literals whose value the port does not keep: every numeric literal,
  in a directive too (`#line 010`, `#pragma warning disable 0168`), and character, string (but a directive's), raw
  string and interpolated text tokens. Only the values that differ from the text are stored, a table searched by id.
  `tree.diagnostics()` is `GetDiagnostics()` in Roslyn's order, errors and warnings alike (`#warning` is CS1030),
  duplicates included: each with its `node`, `code` (`id()` is `CS1002`, `severity()`), byte span `start..end` and
  English `message`; `errors()` and `error_message(id)` give the first message on each node. The text to parse is
  the file's bytes as they are (no `document_text`: a byte order mark is `WhitespaceTrivia`, every line terminator is
  Roslyn's); valid UTF-8 is Roslyn's tree with UTF-16 positions mapped to bytes, other bytes are one U+FFFD each,
  lossless. The tree's text is the source but where Roslyn's tree drops a token so that its positions no longer
  spell the source (one of the 122 741 mutations, a member entry; no corpus file): from there it is the green tree's
  own text, as `ToFullString()` is. No body-less parsing. Its buffers (arena, lexer, token window, list
  builders, the directive, documentation comment and interpolated string parsers', flattener; the tree builder keeps
  none, a tree's columns become the tree) are retained as above: 50.3, 37.3, 49.2 and 9.1 MB after the four corpora
  instead of 101.5, 1245.0, 76.5 and 14.8 without a bound.
- C# syntax getters (`csharp_syntax::syntax_getters`, `src/syntax_getters/`): the properties of Roslyn's syntax nodes
  as free functions over `&Tree` and a `NodeId`, so a consumer asks Roslyn's question instead of matching `NodeKind`s
  in its own order: a module per node type of Syntax.xml and a function per field (`class_declaration::identifier`,
  `method_declaration::return_type`, `using_directive::alias`, `qualified_name::right`,
  `member_access_expression::name`, `invocation_expression::argument_list`, `attribute::name`, ...), a module per
  abstract class with fields on every type deriving from it (`member_declaration::modifiers`,
  `base_type_declaration::identifier`, `type_declaration::members` of a class, struct, record, interface or extension
  block, `base_method_declaration::parameter_list`, `base_property_declaration::type_`,
  `base_namespace_declaration::name` and `usings` of a block or file-scoped namespace), `is_<abstract class>` of a kind
  (`is_type_syntax`, `is_member_declaration_syntax`), `<field>_of` on a node of any type with the field
  (`modifiers_of`, `identifier_of`, `type_parameter_list_of`, `parameter_list_of`, `body_of`, `expression_body_of`,
  `type_of`, ...), `has_modifier(tree, id, TokenKind::StaticKeyword)` (an extension method's `this` on its first
  parameter), `slot_of` and `slot_info` (the slot a child fills and its Syntax.xml field, `Slot::type_name` its
  declared type), `is_in_type_slot` (whether C# reads a node as a type where it stands: `Color Color;`, a cast's type,
  `x is T`'s right side, a base type, a type argument; not a member access's name, a namespace's name or a qualifier),
  and the computed properties of the partial classes (`name::arity`, `get_unqualified_name`,
  `get_alias_qualifier_opt`, `error_display_name`, `type_declaration::arity`,
  `primary_constructor_base_type_if_class`, `parameter::is_arg_list`, `using_directive::name`, `type_::is_var`,
  `directive_trivia::directive_name_token`, ...). A token or node field is `Option<NodeId>`: None where Roslyn's slot
  is null and on a node of another type, `Some` for a missing token (`is_missing`); a list is an iterator of its
  children (a separated list's elements). The flat tree keeps no slots: `slots.rs` reads them back from the kinds in
  Syntax.xml's order (a slot that is never null takes the child where it stands, a separated list takes any token as
  a separator unless the next child fits after a later slot that takes it). Each reads the node's children only up to
  its field, allocates nothing but `name::error_display_name`'s text, recurses nowhere, panics on no tree. They answer
  what Roslyn's own properties answer on every node: `ksdump --lang csharp --getters --check` against
  `tools/csharp-oracle/oracle.sh getters` (the gate's step 13) printed the same lines for the 7064 goldens, the four
  corpora at Latest (62 609 files, 512.6 million lines) and at Preview, the roslyn corpus at `--doc diagnose` and
  `none`, the -defines lists, the 122 741 mutations at their three versions, the 20 000 lexer fuzz inputs at Latest and
  Preview and the 18 975 getters fuzz inputs (1378 million lines in all, 0 differing, 2026-11-04). The boolean fields
  of Syntax.xml are `directive_trivia::is_active`, `branch_taken` and `condition_value` (`Tree::directive_flags`).
- C# reads (`csharp_syntax::read`, `src/read.rs`), what both consumers read beyond the properties and nothing of their
  own facts: `is_trivia`, `is_structured_trivia` (over `psi_builder`'s tree, so they serve as `own_range` and
  `descendants_pruned` predicates), `is_code_token`, `span` (`SyntaxNode.Span`: the full span without the trivia at its
  edges; a missing token at an edge counts, as in Roslyn; `own_range(id, |c| read::is_trivia(tree, c), |_| false)` is
  the span without them), `span_end` (the right edge alone), `first_token` and `last_token` (`GetFirstTerminal`,
  `GetLastTerminal`), `name_leaf` (the identifier of a name's unqualified name), `word` (`ValueText` of a non-missing
  identifier), `dotted_name` (a name's words joined by `.`, without an alias qualifier, type arguments, trivia, `@` or
  escapes: `global::A.List<int>` is `A.List`; unlike `ToString()`, the span's text), `name_segments` (its identifier
  tokens), `text_without_trivia` and `name_text` (the tokens as written without the trivia between them:
  `global::A.List<int>`), `doc_comment` and `declaration_start` (a declaration's documentation comment among its first
  token's leading trivia, parsed or, at `DocumentationMode::None`, a `///` or `/**` comment), `type_names` and `type_head` (the names a type spells, none from type arguments, attributes or function
  pointer parameters). None recurses on a left-deep chain.
- Stack: a parse fits `psi_builder::stack::STACK`, 1 MB of its thread's stack beyond the caller's frames, on every
  target and in every build with optimizations (a debug build's frames are several times larger), by construction:
  at every level it counts (below), it compares the address of a local with the one where its builder started, and
  past `STACK_BUDGET` (768 KiB) it stops as it stops past its depth limit; the rest holds the work below the deepest
  level (the C# lexer's documentation comments nested past their limit, 53 KB). Within one build the result is
  deterministic. Across targets a level's frames differ (the same C# form took 880 bytes a level on macOS arm64,
  1041 on Windows x64), so the depth limit, a count, is the bound wherever it is reached first: every recursive form
  of `tests/robustness.rs` reaches 1000 levels in Kotlin, 800 in Java and 700 in C# before the stack on every target
  CI runs, and below that its tree is the same on all of them. See DESIGN, "Stack".
- Nesting: a Kotlin parse counts the levels of its recursive entry points (expressions, types, member declarations,
  type argument lists, annotations, parenthesized types; a lazy block or lambda adds 4 to where it sits) up to
  `builder::NESTING_LIMIT` (1000), at most `TYPE_ARGUMENTS_LIMIT` (32) nested type argument lists and
  `REPARSED_LIMIT` (10) nested annotations and parenthesized types per block. A construct past a limit or the stack budget is an `ERROR_ELEMENT` with the message
  `builder::TOO_DEEPLY_NESTED`, "Code is too deeply nested", over the rest of the construct (up to the bracket that
  closes it); a block or lambda past the limit is one such error over all its tokens (a block's frame is parsed at
  the driver's depth on the stack, so blocks nest on the heap). The tree stays lossless and in preorder. PSI has no
  defined result there (kotlinc overflows its stack); below the limits the tree is PSI's (the deepest community file
  reaches 62 levels, 5 type argument lists, 4 annotations or parenthesized types).
  Java (`java_syntax::Java`'s `NESTING_LIMIT` 1200, `TYPE_ARGUMENTS_LIMIT` 32, `REPARSED_LIMIT` 5, the same error)
  counts expressions, unary operands, statements, declarations, array initializers, types, type argument lists,
  annotations, patterns and inline doc tags; a code block adds 4 to where it sits. Below the limits the tree is PSI's
  (the deepest community file, `Parenth2.java`, reaches 1010 levels, 4 type argument lists, 4 annotations). See
  DESIGN, "Java nesting".
  C# (`csharp_syntax::Parser`) counts Roslyn's `_recursionDepth` (and the cycles Roslyn leaves unguarded) to 1000,
  measures the stack where Roslyn's `EnsureSufficientExecutionStack` does, and counts every consumed token against
  `green::work_budget_for` (16 a byte, at least 2^20); past any of them, or past the arena budget
  (`green::budget_for`, 256 bytes a source byte at fixed record sizes, checked where buffers grown from empty would
  grow, so it depends on the file alone), the tree is Roslyn's for an exhausted stack: the whole text one skipped
  `BadToken` with `ERR_InsufficientStack`, as Roslyn returns at 100 000 levels. Below them the tree is Roslyn's (the
  corpora reach 678 levels and 1.0 token a byte). See DESIGN, "C# nesting and work".
- Allocator: psi-syntax uses the system allocator and builds no C. mimalloc 0.1.52 was +12-15 % at 8 and 16
  threads before batch 2's buffer reuse; on the current code it changes no instructions and its wall time is within
  noise at 1 and 16 threads (DESIGN, "Build and toolchain"), so a consumer needs no allocator for the parse.
- Build: the consumer's release profile compiles psi-syntax. Fat LTO and one codegen unit, as accio's `psi` profile
  (`cargo build --profile psi -p ksdump`, the binary `../target/psi/ksdump`) that every measurement here uses; a profile-guided
  build (`scripts/pgo.sh` is the recipe: instrument, run over a few MB of Kotlin and Java, merge, rebuild with the
  same rustc; in a consumer's own build `RUSTFLAGS=-Cprofile-generate=DIR`, a run, `llvm-profdata merge`,
  `RUSTFLAGS=-Cprofile-use=merged.profdata`) cuts 12 % of Kotlin's and 13 % of Java's instructions on 72c8c11
  (goldens as training; ×1.10 and ×1.13 wall on one thread; other real code as training did 1-4 points better on the
  hints commit's first base) with byte-identical trees; `panic = "abort"` another 1-2 %. Measured in DESIGN, "Build
  and toolchain" and "Current".

## The dump format (`DebugUtil.psiToString(file, showWhitespaces = true, showRanges = false)`)

First line `KtFile: <file name>`, then each node indented two spaces per depth: a composite prints its element type
(`PROPERTY`), an empty composite gets a child line `<empty list>`, a leaf prints `PsiElement(<token debug name>)('<text>')`,
whitespace `PsiWhiteSpace('<text>')`, a comment `PsiComment(EOL_COMMENT)('<text>')`, an error `PsiErrorElement:<message>`.
Texts escape `\n`, `\t`, `\r` and `\\`, `'` as `DebugUtil` does. The whole dump is trimmed.

## Oracles

1. `cargo nextest run --release -p kotlin-syntax --test goldens --no-capture`: the 789 vendored goldens of the
   compiler's own parsing tests (Apache-2.0, see NOTICE). The first gate; run it after every change.
2. The live PSI oracle (kotlin-compiler-embeddable 2.4.21) on real code:
   `java -Xss512m -Xmx8g -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" $HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/PsiOracle.java --list LIST --root ROOT --out OUT`
   writes `OUT/<path>.psi.txt` and `OUT/index.jsonl` (`path`, `psiErrors`, `firstError`). Ready dumps:
   `$HOME/.cache/psi-syntax/oracle/community-3caa0bf/` for the list `$HOME/.cache/psi-syntax/corpus/community-3caa0bf.list`
   over `/Users/develar/.cache/accio-bench/intellij-community` (55 501 files). Compare with
   `scripts/compare.py ORACLE_DIR OURS_DIR`.
3. The lexer oracle: `java -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" scripts/LexOracle.java --list LIST
   --root ROOT` prints a hash of each file's tokens (Kotlin, and KDoc inside each doc comment) as `ksdump --lex` does;
   `diff` the `cut -f1,2` of both. `scripts/lex-fuzz.py` makes mutated inputs for it.
4. The getter oracle: `java -Xss512m -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*"
   tools/kotlin-psi-getters/GetterOracle.java --list LIST --root ROOT (--out OUT | --hash FILE)` prints what Kotlin's PSI
   getters answer for every element of each file (the lines of `tests/common/getters.rs`);
   `target/release/examples/psi_getters` (`cargo build --release -p kotlin-syntax --example psi_getters`) prints ours
   with the same flags: `diff` the hash files, then `diff -r` the `--out` lines of a differing file. A reduced case
   goes to `crates/kotlin-syntax/tests/psi/` with the oracle's `NAME.kt.getters.txt`; a changed golden regenerates
   `tests/psi/goldens.hash` (`--hash` over the goldens' list, paths relative to `testdata/`).
5. Error recovery: `scripts/parse-fuzz.py LIST ROOT OUT [SEED]` mutates files at token boundaries (punctuation,
   keywords, modifiers) for the PSI oracle and `scripts/compare.py`; `scripts/lex-fuzz.py`'s inputs serve it too.
   `scripts/psi-reduce.py FILE OUT` shrinks a differing file to a small input; a fixed case goes to
   `crates/kotlin-syntax/tests/oracle/`
   (`NAME.kt` and the oracle's dump `NAME.txt`), which `cargo nextest run --release -p kotlin-syntax --test oracle`
   compares.

Java: `testdata/java/` (1036 goldens of IntelliJ's Java parsing tests, `manifest.tsv` with level, entry and trim per
test) and the Java PSI oracle `tools/java-oracle/JavaPsiOracle.java` (see `tools/java-oracle/README.md`: cached dumps of
the 68 263 community `.java` files, 19 116 golden mutations and 20 000 lexer fuzz inputs; with `--getters` the answers
of the PSI getters `java_syntax::psi` ports, cached in `$HOME/.cache/psi-syntax/getters/2/`). The Java lexer oracle
`cargo run --release -q -p ide-oracle --bin java-oracle -- lex --list LIST --root ROOT` (`JavaLexOracle.java` on the
pinned jars) prints the lines `ksdump --lex` prints for `.java` files (JavaDoc tokens of each doc comment inside `[`
`]`); `java-oracle lex-fuzz` makes lexer fuzz inputs for it. Cached lines: `$HOME/.cache/psi-syntax/lex/`.
`java-oracle` (`crates/ide-oracle`, the oracle harness: pinned jars, the IDE's JVM, comparison, goldens, Python's
seeded generator) also vendors and checks the goldens, lists the community files and writes the mutations; its
README is `tools/java-oracle/README.md`. The oracles' tooling is Rust; the oracle programs stay Java or C#.

C#: `testdata/csharp/` (7064 goldens of Roslyn's parsing and lexing tests) and `tools/csharp-oracle` (Roslyn 5.9.0
itself: `oracle.sh dump`, `oracle.sh lex`, `oracle.sh values` and `oracle.sh getters`, the cached dumps, lexer lines
and getter lines of four corpora, 122 741 golden mutations, 20 000 lexer fuzz inputs and 18 975 getters fuzz inputs); "C#" above has the commands, `tools/csharp-oracle/README.md` the format.
Reduced cases go to `crates/csharp-syntax/tests/oracle/`.

Never edit a golden or an oracle dump to make a test pass. A difference is a bug in the port until the Java (or
Roslyn) says otherwise.

## Porting conventions (the builder and parser API)

- What is the platform's goes to `psi-builder`, generic over `L: Language`; what is the language's goes to its crate,
  and reaches the builder only through `Language` (a new need of a parser is a new `Language` item or a new generic
  builder method, never a language's token inside `psi-builder`). Monomorphized, the generic code is the code it was:
  the split was measured at no cost (DESIGN, "Measurements").

- `AbstractKotlinParsing` is a trait implemented by `KotlinParsing<'b>` and `KotlinExpressionParsing<'p, 'b>`; its
  helpers are provided methods (`self.at(LBRACE)`, `self.expect_with(RPAR, "Expecting ')'")`). `myBuilder` is
  `self.my_builder()`, a `SemanticWhitespaceAwarePsiBuilder` (the Java builder seen through its adapters).
- `KotlinExpressionParsing` is a borrow of its `KotlinParsing`: `myExpressionParsing.f()` is
  `self.my_expression_parsing().f()`, `myKotlinParsing.g()` is `self.my_kotlin_parsing.g()`.
- A marker is a `Copy` handle whose methods take the builder: `marker.done(self.my_builder(), FUN)`,
  `marker.precede(self.my_builder())`, `marker.error(self.my_builder(), "...")`, `rollback_to`, `drop`, `collapse`,
  `done_before`, `error_before`, `set_custom_edge_token_binders(b, Some(left), None)`.
- A token type is `Option<TokenKind>` (`null` = `None`) and compares with a token directly: `self.tt() == LBRACE`. A
  `TokenSet` is a `const`, `TokenSet::create(&[..])`, `or_set`, `and_set`, `and_not`; `contains` takes a token, a node
  type or an `Option`. Composite types are `NodeKind`; an `IElementType` result is `Option<NodeKind>`.
- Overloads get suffixed names: the fewest-argument form keeps the name, the others `_with` (or a descriptive suffix,
  e.g. `expect_with_recovery`, `parse_type_ref_with_intersections`), documented on each function.
- Every stub says `TODO(port): <File>.java:<line>` (Java: `<File>.kt:<line>`, C#: `<File>.cs:<line>`); port the method
  there, keep the order.
- Java: a parser method's `builder: SyntaxTreeBuilder` is `builder: &mut SyntaxTreeBuilder<'_>`
  (`java_syntax::builder`), its sub-parsers views reached through `self.my_parser` (`JavaParser`); a parameter named
  `type` is `element_type`; a Kotlin method that reaches the builder only through a marker takes the builder first
  (`java_parser_util::done(builder, marker, NodeKind::CLASS, level)`); a default parameter gives the short name the
  default and `_with` the full form (`parse_code_block` / `parse_code_block_with`). Token parameters are
  `Option<TokenKind>`, node types `NodeKind`, either `ElementType`.

## Rules

- Dependencies: a well-maintained crate is better than a reinvented one when it is measurably as fast or faster
  and keeps the output byte-identical; each one is justified in its commit (what it replaces, the measured effect,
  its maintenance). Nothing that needs a C toolchain or pulls a large tree. No `unsafe` of our own (the one exception
  is `ab-pairs`'s `sys`, the measurement tool's reading of the kernel's records of a child), no `unwrap` on
  input-dependent paths: any byte sequence parses (invalid UTF-8 is lexed as BAD_CHARACTER bytes, in C# as one U+FFFD
  code unit a byte) and never panics.
- Speed is a goal: no allocation per token beyond the arena, no `String` per leaf (texts are ranges), no recursion
  depth that a 4 MB file of nested expressions can overflow without a guard (PSI's own limits apply).
- `scripts/pgo.sh OUT_DIR [KOTLIN_LIST KOTLIN_ROOT JAVA_LIST JAVA_ROOT]` builds a profile-guided ksdump (training on
  the vendored goldens by default) and leaves the merged profile for a consumer's build.
- Measure speed with `cargo run --release -p ab-pairs --bin ksdump-ab -- OLD_KSDUMP NEW_KSDUMP [-- BENCH ARGS]`:
  `ksdump --bench` of each in interleaved pairs until the stopping rule's verdict, under the host's measurement
  reservation (the A/B engine of accio's `scripts/bench`, decision 2026-11-06-ab-pairs-in-psi-syntax). Where the host
  counts them, instructions retired decide (they move far less than MB/s on a loaded machine), guarded by the parse
  time and cycles per instruction; `--metric time` judges the parse seconds. `--list`/`--root` pick the corpus
  (default the community `.kt` files). Profile with `scripts/profile.py` (macOS `sample`, symbolized inline-aware with
  `atos -i`: self time per function and per source line, inclusive time per source function).
- `scripts/gate.sh` prints PASS before every commit: clippy and `KS_STRICT=1 cargo nextest run --release --profile
  heavy` (then the doc tests) over psi-syntax's crates, byte equality of
  all 55 501 community dumps and of the cached fuzz corpora with the PSI oracle, `ksdump --check-bodies` on the
  community files, and again with `--nesting-limit 3` on the community files and the mutations, and the Java lexers
  against the Java lexer oracle on the community `.java` files, the golden mutations and the lexer fuzz, and both Java
  dumps (`.psi.txt`, `.node.txt`) of the same three corpora against the Java PSI oracle's, the `java_syntax::psi`
  getters (`ksdump --getters --check`) on the goldens and those corpora against its `--getters` lines, and `ksdump --check-bodies`
  on the community `.java` files and with `--nesting-limit 3` on them and the golden mutations, and the C# lexer
  (`--doc none`, and `--doc parse` and `--doc diagnose` with the documentation comment parser) against
  `tools/csharp-oracle`'s cached lexer lines on its four corpora, mutations and lexer fuzz, the C# value texts
  (`ksdump --values --check`) against its cached `values --kept` lines of the same inputs, and the C# parser's dumps
  against its cached dumps: the four corpora at Latest and at Preview, the mutations at their goldens' versions, at
  Latest and at Preview, the lexer fuzz at Latest and at Preview (the 6 runtime files without a dump only lossless),
  and the dumps and lexer lines of the roslyn and Avalonia files whose conditions name a symbol, every one defined,
  and the C# directives' flags (`ksdump --directives --check`) against its cached `directives` lines of all of those,
  and the C# syntax getters (`ksdump --lang csharp --getters --check`) against its cached `getters` lines.
  A faster change that fails it is not a change. The profile is what runs `tests/robustness.rs` of the three
  languages: accio's `default-filter` (`../.config/nextest.toml`) leaves those binaries out of every other run, the
  gate and CI's `heavy` job being their only homes (decision 2026-11-07-heavy-parser-tests-opt-in). A bare `-E
  'binary(robustness)'` selects nothing; `--profile heavy` or `--ignore-default-filter` is how they run.
- Java speed: `ksdump-ab OLD NEW --list $HOME/.cache/psi-syntax/corpus/java-community-3caa0bf.list` (the community
  `.java` files); DESIGN, "Java parse throughput", has the numbers and how tree-sitter-java was compared.
- C# speed: `ksdump-ab OLD NEW --list $HOME/.cache/psi-syntax/corpus/csharp-roslyn-35d9211.list --root
  $HOME/.cache/psi-syntax/csharp-src/roslyn` (any of the four corpora; dotnet/runtime's time is bound by
  `hugeexpr1.cs`); DESIGN, "C# parse throughput and robustness", has the numbers and how tree-sitter-c-sharp was
  compared.
- A change lands as any accio change does: a pull request to accio's `main`, a conventional subject
  (`fix(psi): …`, `perf(psi): …`; `../AGENTS.md`, "Writing a change down"), a body that says what the change does,
  its measured effect and that the gate passed. A parser change that moves what accio extracts carries accio's version
  bump and snapshots in the same pull request. Its commits before the move came from develar/psi-syntax, replayed as a
  line under `psi-syntax/`; that repository is now the public mirror, and after the merge
  `../scripts/psi-syntax-mirror.sh` replays the commit there for Branch Deck.
