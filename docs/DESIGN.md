# psi-syntax: design

AGENTS.md says what to port and how to check it; this document says how the port is built, why, what it costs and what
is still open. The design sections are in the present tense. Every number comes with the commit it was measured on
and the date.

## Contract

For every byte sequence, `dump::psi_to_string(&tree, name)` prints exactly what Kotlin 2.4.21's
`DebugUtil.psiToString(file, true, false).trim()` prints, and the parse never panics. `scripts/gate.sh` checks this
before every commit:

- clippy, on default features and with `counters,alloc-stats`;
- `KS_STRICT=1 cargo test --release`;
- byte equality with the PSI oracle on all 55 501 community files and on the cached parse-fuzz and lex-fuzz corpora;
- `ksdump --check-bodies`: each of the 229 960 community bodies parsed on its own equals the full parse's subtree;
- the same with nesting limit 3 (`--nesting-limit 3`) on the community files and the mutations, so the too-deep paths
  run on ordinary code: every tree lossless, every body exact.

For Java the contract is the same against IntelliJ 271's Java PSI: `java_syntax::dump::psi_to_string` prints
`DebugUtil.psiToString(file, true, false).trim()` and `node_tree_to_string` the `_node.txt` dump. The gate checks both
dumps on the 1036 goldens (`KS_STRICT=1`), the 68 260 UTF-8 community `.java` files, the 19 116 golden mutations and
the 20 000 lexer fuzz inputs, and the Java lexers' token lines on the same corpora (see "Java", "Oracle"); then
`ksdump --check-bodies` on the community `.java` files (443 161 code blocks), and with `--nesting-limit 3` on them and
on the golden mutations.

For C# the contract is the same against Roslyn 5.9.0 (`Microsoft.CodeAnalysis.CSharp`, dotnet/roslyn 35d9211b):
`csharp_syntax::dump::tree_to_string` prints what `tools/csharp-oracle` prints of the syntax tree Roslyn returns, its
nodes, tokens, trivia, missing tokens and diagnostics with their messages, at UTF-8 byte offsets. The gate checks it
on the 7051 goldens of Roslyn's own parsing and lexing tests and 4 reduced cases (strict), on the 62 615 valid UTF-8
`.cs` files of dotnet/roslyn, dotnet/runtime, dotnet/aspnetcore and Avalonia at Latest and at Preview, on 122 741
golden mutations at their goldens' versions, at Latest and at Preview, and on 20 000 lexer fuzz inputs at Latest and
at Preview; the lexer's tokens alone at the three documentation modes on the same inputs; the 6 runtime files whose
dump is too large for the oracle only lossless (see "C#").

Speed work therefore changes only three things, none of which changes a parser decision:

- how the cursor and the markers are represented;
- when and what is allocated;
- the layout of the finished tree.

Before and after each such change, the 25 `--features counters` counters (cursor calls, markers, rollbacks, lexed,
reused and skipped tokens) are compared. The token cache was meant to move only the raw cursor-call counts, and the
brace skip only the calls, markers and tokens skipped inside lazy blocks. Every other change left all 25 identical.

## Workspace

The crates are members of accio's Cargo workspace (`psi-syntax/crates/*`, since 2026-11-03; before, the repository
github.com/develar/psi-syntax), with their own package fields and lints; accio's `crates/extract` depends on them by
path.

| crate | holds | depends on |
|---|---|---|
| `psi-builder` | `PsiBuilderImpl<L>`, `Marker`, the `WhitespacesAndCommentsBinder` contract, `Tree<L>` and `Parser<L>` (the driver), `TreeData`, the dump's layout, `ElementTypeSet`, `Lexed<T>`, `char_at`, `in_ranges`, the builder's counters, the retained buffers (`buffers`: `Shrink`, `RETAINED_BYTES`) | nothing |
| `kotlin-syntax` | `TokenKind`, `NodeKind`, `TokenSet`, the Kotlin and KDoc lexers, `Binder`, `SemanticWhitespaceAwarePsiBuilder*`, the parsers, KDoc, `Kotlin: Language` | `psi-builder` |
| `java-syntax` | `TokenKind`, `NodeKind`, `TokenSet`, the Java and JavaDoc lexers, `Binder`, `SyntaxTreeBuilder` (the Java parsers' builder handle), `Java: Language`, the dumps, the parsers | `psi-builder` |
| `csharp-syntax` | `SyntaxKind`, `TokenKind`, `NodeKind`, the generated node types and diagnostics, the lexer and directive parser, `SyntaxParser`, `LanguageParser`, `DocumentationCommentParser`, the green arena, `flatten`, `CSharp: TreeLanguage`, `Tree` with diagnostics, the dump | `psi-builder` |
| `ksdump` | the binary | `kotlin-syntax`, `java-syntax`, `csharp-syntax` |

`psi-builder` is generic over `L: Language`, a marker type whose associated items are what IntelliJ reads from a
`ParserDefinition` and from element types: `Token`, `Node` (each with a dense `u16` index and a debug name; node indices start at `TokenType::NODE_BASE`, 256 for Kotlin and Java),
`Binder`, `Lazy` (what a chameleon is reparsed with), `FileKind`, `Frame` (a builder with the language layers' state over it, `frame_builder`, `frame_builder_ref`),
`FILE`, `ERROR_ELEMENT`, `is_whitespace_or_comment`, `is_left_bound`, `is_chameleon_token`, `chameleon`, `is_body`,
`joined_token_type` (the cursor cache's derived token), `bracket_depth_change` (the too-deep skip), `tokenize_into`,
`parse_file` and `parse_chameleon`. `tokenize_into` gets the file's `FileKind` and `Chameleon::file_kind` gives it to
`parse_chameleon`, because Java's lexer and parsers depend on the file's language level. The driver hands `parse_chameleon` a `Chameleon`: the chameleon's text and three
ways to a builder (the body's file tokens when they lex alone the same, else relexed; relexed; the caller's own tokens).
`kotlin_syntax::{Tree, Parser, PsiBuilderImpl, ElementType}` are aliases of the generic types for `Kotlin`, so the
Kotlin parsers and consumers name them as before.

`Token`, `Node`, `FileKind` and `ERROR_ELEMENT` form the supertrait `TreeLanguage`: all that `Tree<L>`, its consumer
API and the dump's layout need. A language whose parser does not run on `PsiBuilderImpl` (C#, ported from Roslyn,
which builds green nodes bottom-up and hangs trivia on tokens) implements only `TreeLanguage` and fills a tree in
preorder through `TreeBuilder<L>` (`open`, `leaf`, `close`, `set_trailing`, `set_missing`, `set_error`, `finish`, or
nodes written by id into columns sized up front: `writer`, `ColumnWriter`, `truncate`). The split changed none of the
25 counters on Kotlin or Java.

Everything is monomorphized per language; nothing is `dyn`. A function the Kotlin crate's own code used to inline may
be instantiated differently from another crate: `skip_whitespace`, inlined before, became a call on every cursor move
after the split and cost 0.8 %, so it is `#[inline(always)]`; Kotlin's `getJoinedTokenType`, a call before, is again
not forced inline. The split's measurements are under "Measurements".

The inlining hints follow what a profile-guided build (`-Cprofile-use`, "Build and toolchain") inlines, so that a
plain build gets part of it. `#[inline(always)]`: the small builder methods the parsers call at hundreds of sites (`mark`, `precede`,
`get_current_offset`, `look_ahead`, `get_token_bytes`, `process_done` into `done`), Java's `expect`, `expect_set`,
`current_offset`, `look_ahead`, `token_text` and `java_parser_util::done`, Kotlin's `look_ahead`, `get_token_bytes`,
`get_current_offset` and `join_complex_tokens`. `#[inline]`: `allocate`, `create_marker`, `free_marker`, `done`,
`balanced_block_end_at`, `shift_over_whitespace_forward` and the Java builder's raw lookups. `char_at` and Java's
`[:jletter:]`/`[:jletterdigit:]` inline their ASCII test and call out of line, `#[cold]`, above ASCII. Every method
that reports an error, and the too-deep skips, are `#[cold]`: a profile knows those branches are rare, LLVM's
default weights do not. Forcing `done` or `allocate` inline as well cost 1.5 % more instructions on both languages;
outlining `current_types`' recompute cost 0.6 %; hints on the Kotlin lexer's helpers changed nothing (measured in
"Inlining as a profile does it").

## Pipeline

```
text ─ Language::tokenize_into ─> Lexed {types, starts}   (file tokens, Parser-owned buffers)
     ─ PsiBuilderImpl + KotlinParsing ─> production        (markers, one Rust fn per Java method)
     ─ begin_bind: balance_white_spaces, bind ─> TreeData   (Parser-owned columns, preorder)
         └─ at each chameleon (BLOCK, LAMBDA, KDoc, markdown link): suspend the frame, parse it, bind, resume
     ─ copy out ─> Tree {Arc<[u8]>, kinds, ranges, links, errors}   (exactly sized, Send + Sync)
```

### Lexer (`kotlin-syntax`: `src/lexer.rs`, `src/kdoc/lexer.rs`)

The lexers are hand ports of `Kotlin.flex` and `KDoc.flex`, not generated code:

- the same states and state stack (with `lBraceCount` and the interpolation prefix);
- longest match, and the earlier rule on a tie;
- `yypushback` where the spec uses it.

Offsets are bytes. A byte that does not start valid UTF-8 counts as one character. Character classes above ASCII come
from tables decoded out of the generated Java lexers' character maps (`src/unicode.rs`), so `[:letter:]` is Java's
own. ASCII is handled by byte tables:

- `tokenize_into` (what a parse lexes) dispatches on a 256-entry class of a token's first byte while the state is one
  of `lex_inclusive`'s and the byte cannot change it (all but `"`, `$`, a backtick, and a brace in
  LONG_TEMPLATE_ENTRY): identifiers, hard keywords, whitespace and one-byte tokens are lexed inline, a complete
  `"…"` without escapes or templates as its two or three tokens, the rest by `lex_inclusive`; anything else goes
  through `advance`. The plain bit of such a token is the one `advance` left, since none of them changes the state.
- Hard keywords are a perfect hash (`hard_keyword_hash` of the first and last byte and the length into 64 slots, a
  collision fails to compile), one candidate compared as a masked word read from the text (`psi_builder::lexer::
  Keyword`); `TokenKind::soft_keyword_by_text` / `keyword_by_debug_name` are byte matches.
- Identifier parts scan `ASCII_IDENTIFIER_PART`; a byte at 0x80 or above is decoded and asked `is_identifier_part`.
- Tokens go to `Lexed` through `LexedWriter`, a batch of 64 on the stack (two stores and a bit a token), appended
  when full.
- KDoc (`src/kdoc/lexer.rs`) tries, of a state's rules, only those that can start with the byte at hand
  (`STATE_RULES & FIRST_BYTE_RULES`, bit sets in spec order). In CONTENTS (and in CONTENTS_BEGINNING at a byte that
  starts no other rule) a run of text and whitespace is one KDOC_TEXT token, which `KDocLexer` would merge from
  `ContentsAny`'s one-character tokens and `ContentsWhiteSpace`'s runs; the run applies their actions once. A line's
  break, leading whitespace and asterisks, and CONTENTS_BEGINNING's whitespace take their rule directly
  (`line_start`).

Java's `tokenize_into` does the same over `JavaLexer` (see "Java"). The lexers' tokens and plain bits were compared
with the previous code's on every gate corpus before the change; the gate's lexer oracles hash `tokenize_into`'s
tokens.

### Builder (`psi-builder` `src/builder.rs`, Kotlin's layer in `kotlin-syntax` `src/builder.rs`)

`PsiBuilderImpl` is ported with IntelliJ's semantics:

- the `MarkerProduction` order;
- rollback to a marker's production entry, freeing markers from the last entry back;
- the global `lastErrorIndex` error dedup;
- the edge binders, `remapCurrentToken` and `tokens_to_restore_on_rollback`.

Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl` adds the newline-mode and token-join stacks on top. The adapters
(`TruncatedSemanticWhitespaceAwarePsiBuilder`, `ForByClause`) are a stack inside the builder, seen through a handle,
`SemanticWhitespaceAwarePsiBuilder`.

What the port adds to the hot path:

- **A current-token cache** in `PsiBuilderImpl`, computed at every move rather than at the first read: `sig` (where
  `eof()` puts the cursor: past whitespace and comments after a move, the cursor itself after `rollbackTo`, which sets
  `myTokenTypeChecked`), `tt_raw` and `tt_joined` (`Language::joined_token_type`, Kotlin's `getJoinedTokenType` body).
  - `eof()`, `getTokenType()` and `current_token` (the layer's type: joined while Kotlin joins complex tokens, told
    on every push and pop of that stack, `set_joined`) are a store of `sig` into the cursor and a read, without a branch;
    the cursor itself moves over whitespace only where the Java's does (`eof()`, `mark()`), so `rawTokenIndex`,
    `rawLookup` and the lexemes markers record are the Java's.
  - It is recomputed wherever the cursor moves or `lex_types` changes: `advance_lexer`, `raw_advance_lexer` (and
    `advance_to_eof`), `rollback_to` (which also restores remapped tokens), `remap_current_token`, `remap_tokens`,
    `skip_balanced_block`, and `mark()` when it steps over whitespace `eof()` would not (after a rollback).
  - `myTokenTypeChecked` is kept as `checked` only for a remap: a token at the cursor remapped into whitespace or a
    comment is skipped only while it is false. `current_token` does not record it (a store on the hottest path), so
    a layer reading the cursor through it never remaps into whitespace (Kotlin remaps to soft keywords and
    identifiers only).
  - The cache lives in `PsiBuilderImpl`, not in the semantic-whitespace layer, because `Marker` methods mutate
    `PsiBuilderImpl` directly.
- **The semantic layer's stack tops** (`join_top`, `newlines_top`) are fields updated on push and pop, so
  `getTokenType()` reads a bool, not `Vec::last`; `newlineBeforeCurrentToken` walks the raw tokens before the cursor
  as a slice.
- **The handle caches** `top_trunc` and `min_eof` (the smallest non-negative `eof_position`). It sets them in
  `handle()`, `reborrow()` and `wrapped()`. This is sound by the borrow checker: `wrapped(self)` consumes the handle
  and `reborrow` freezes its parent, so the adapter prefix cannot change while the handle is alive.
- **Token text is compared as bytes** (`get_token_bytes`, which returns `b"?:"` and `b"?."` for joined tokens) in
  `at`, `at_set`, `expect` and the keyword helpers. Only a test calls `get_token_text`, so `str::from_utf8` is off the
  hot path.
- **`#[inline(always)]` on the cursor chain** (`tt`, `_at`, `token_matches`, `_at_set`, `at`, the handle's and the
  impl's `get_token_type`/`eof`).
- **`skip_balanced_block(open, close)`.** When no `Truncated` adapter is active, `advance_balanced_block` counts
  `LBRACE`/`RBRACE` over the raw `lex_types`, instead of the Java loop that joins `?.`/`?:`/`!!` and creates their collapse markers.
  The result is the same:
  - Both callers collapse the enclosing marker: `parse_block_with` collapses `BLOCK` and the lazy lambda collapses
    `LAMBDA_EXPRESSION`.
  - `bind` skips a collapsed marker's children.
  - The markers it leaves out are monotone in lexeme, so they move no `balance_white_spaces` edge.

  Under a truncation the Java loop runs unchanged.

Marker storage:

- **`MarkerData`** is 20 bytes. It holds kind, collapsed, the binders, element type, lexeme, done lexeme and the
  nesting where it was collapsed. An error message lives in a side table and is addressed by a `u32`. There are no
  light-tree links.
- **The production is a doubly linked list while parsing**, not Java's `IntArrayList`. `links[node]` is
  `[prev, next]`; node `2·id` is marker `id`'s start entry and `2·id + 1` its done entry, so `indexOf` is the marker's
  own node and `precede`, `doneBefore`, `errorBefore`, `drop` and `rollbackTo` touch only the entries they add or
  remove. Java inserts into and removes from the middle of its array (`ArrayList.add(index, …)`), and finds an entry
  by a linear window, a binary search by lexeme and a walk over the markers at that lexeme; a chain `a+a+…` preceded
  the same far-back marker at every operator, quadratic: 4.67 s for 100 000 operators on c796e73. The order of entries,
  and so the tree, is the array's. A marker done twice (Java only logs it) gets an extra done node; while one is
  linked, `drop` finds a marker's last done entry by walking back, as `lastIndexOf` does.
- **`entries`** is the list made into one array by `prepare_light_tree` and balanced in the same walk
  (`flatten_and_balance`): each entry's id, its balanced lexeme and, for a start entry, its element type and whether
  it is collapsed, so `bind` reads one contiguous array and no marker on its common path. The walk reads each lexeme
  from its marker before balancing writes it, as the Java's array does, because while no extra done node is linked
  each lexeme field belongs to one entry; with one linked, the array is made first and balanced after. The default
  binders take a shortcut exact by their contract: `DEFAULT_LEFT_BINDER`'s edge is the whitespace's end and
  `DEFAULT_RIGHT_BINDER`'s its start, whichever of the Java's three cases (a token view of positive, negative or
  zero size) applies, so neither builds a view.
- **`bind` walks the production directly**, with no stack and no `prepareLightTree` links, in one loop over the
  entries (leaves, then open or close a composite or append an error), leaving its state machine only at a
  chameleon leaf, a collapsed marker and the end. The open composites are the current node's ancestors in the tree
  being built: closing one reads its parent from the tree, and each open composite's marker is stashed in its
  subtree end (`TreeData::stash`) until closing overwrites it; the root's marker is `BindState`'s. It reproduces
  `prepareLightTree`'s rules:
  - any done closes the current marker at its own done lexeme;
  - a done at the root closes nothing, so entries after the root's done become root children;
  - error items inside collapsed markers count for the global dedup, although their entries are skipped;
  - open markers close innermost first.

### Java (`java-syntax`)

The sources are java-syntax and platform/syntax of JetBrains/intellij-community at decc943 (AGENTS.md, "Java").

- **Lexers.** `lexer.rs` is `JavaLexer.kt` (its own dispatch on the located character: whitespace, `{`/`}` with the
  template brace counts on its state stack, comments, character and string literals, text blocks, templates, `#!`,
  `\u001a`; `locateCharAt` decodes unicode escapes, `mySymbolLength` their length) over `FlexJavaLexer`, the rules of
  `_JavaLexer.flex` matched as JFlex matches them (longest match, the earlier rule on a tie: the numeric rules are
  computed alternative by alternative). `doc_lexer.rs` is `JavaDocLexer.kt`: the merging adapter over
  `AsteriskStripperLexer` over `FlexJavaDocLexer`, each of `_JavaDocLexer.flex`'s 14 states a list of its rules in
  spec order. Offsets are bytes; the code compares characters only with ASCII ones, so boundaries are those of the
  UTF-16 original. `[:jletter:]` and `[:jletterdigit:]` above ASCII are decoded from the generated lexers' DFA tables by
  running them (`tools/java-unicode.py`), JFlex's `.` excludes `\n\r\u000B\u000C\u0085\u2028\u2029` (read off the
  same tables).
- **Lexer fast paths.** `locateCharAt` of an ASCII byte other than a backslash is that byte (inline; the decoding is
  `locate_char_at_slow`), and an ASCII byte is never inside a multi-byte character, so the scans of comments, line
  comments, literals and whitespace skip the bytes that cannot end them (eight at a time for comments:
  `psi_builder::lexer::find_either`, `find_any3`) and decode only at a backslash or a byte above ASCII.
  `tokenize_into` dispatches on the class of a token's first byte (`BYTE_CLASS`): an ASCII letter, digit, whitespace,
  one-byte token or operator is lexed without `locateToken` (what it would do with that byte: the state stack is
  only read by `{` and `}`, which take the fast path while its top counts no template braces); a backslash, a byte
  above ASCII, `/`, the quotes, `#`, `\u001a` and the controls go through `locate_token`. Keywords are a perfect hash
  of the first, second and last byte and the length into 128 slots (`keyword_hash`), compared as one or two words.
  Outside markdown mode, `FlexJavaDocLexer` lexes a run of COMMENT_DATA as one DOC_COMMENT_DATA token
  (`comment_data_run`): every rule that matches inside it returns DOC_COMMENT_DATA and stays in COMMENT_DATA, so the
  merging adapter would join its tokens into the same one.
- **Builder.** The Java parsers run on platform/syntax's `SyntaxTreeBuilder`; the port runs them on `psi_builder`'s
  `PsiBuilderImpl` seen through `SyntaxTreeBuilder<'b>`, a handle with the `stopAt` offset of
  `JavaParserUtil.stoppingBuilder` (nested ones stop at the nearest), so no adapter is a type of its own and
  `DeclarationParser.parse` recursing on a stopping builder does not instantiate generics without end.
  `enforceCommentTokens({DOC_COMMENT_LEADING_ASTERISKS})` is folded into `Java::is_whitespace_or_comment`: that token
  occurs in no other builder and the Java comments in no doc comment's. `lastDoneMarker` and the marker accessors
  (`getNodeType`, `getStart/EndOffset`, `getStart/EndTokenIndex`) are generic `psi_builder` methods.
- **Lazy elements.** `CODE_BLOCK` is a body (collapsed by `parseBlockLazy`, reparsed by `parseCodeBlockDeep` at the
  file's level); `DOC_COMMENT` and `DOC_MARKDOWN_COMMENT` (Java lexer tokens) are lexed with `JavaDocLexer` and parsed by
  `JavaDocParser`; `DOC_REFERENCE_HOLDER` and `DOC_TYPE_HOLDER` (JavaDocParser remaps or collapses into them) are lexed
  with `JavaLexer(JDK_1_3)` (the type's through `JavaTypeEscapeLexer`). The four token-typed ones are `TokenKind`s, as
  Kotlin's KDoc is.
- **Files and entries.** `FileKind { level, entry }`: a file (`java.FILE`) or one of the 13 parser entries of the
  parser-partial goldens under `test.java.file` with "Unparsed tokens" over what is left, as
  `JavaParsingTestConfiguratorBase` runs them.
- **Dumps.** `node_tree_to_string` prints the element types (`DOC_SPACE` as PSI's `WHITE_SPACE`); `psi_to_string` the
  PSI class of each node and its `toString()` (`NodeKind::label`, from the goldens' 1036 dumps). Where a `toString()`
  computes its text, the dump ports that computation: `PsiPackageStatementImpl.getPackageName()` is
  `JavaSourceUtil.getReferenceText` (`push_reference_text`), the module name
  `PsiJavaModuleReferenceElementImpl.getReferenceText()`, and a `PsiInlineDocTag` without a name prints `null`.
- **Oracle.** `tools/java-oracle/JavaPsiOracle.java` runs IntelliJ 271's real Java PSI (java-syntax converted to PSI by
  `PsiSyntaxBuilderFactoryImpl`, as the IDE does) from 23 IDE jars pinned by sha256 (`fetch-jars.sh`), on a
  `CoreApplicationEnvironment` with the parsing part of `JavaCoreApplicationEnvironment`; a list line carries the
  level (set as `FILE_LANGUAGE_LEVEL_KEY`, as the tests do), the entry and the trim of each file. It reproduces all
  1036 vendored goldens of IntelliJ's Java parsing tests byte for byte, both dumps, which is what makes its dumps of
  other corpora the reference. `JavaLexOracle.java` prints `JavaLexer`'s and `JavaDocLexer`'s tokens as
  `ksdump --lex` does. Its cached dumps are listed in `tools/java-oracle/README.md`.
- **Bench.** `ksdump --bench` takes a list of Java files (`PATH[<TAB>LEVEL]`, every one a file), on default thread
  stacks; `--check-bodies` takes one as `--out` does (`PATH[<TAB>LEVEL<TAB>ENTRY<TAB>TRIM]`).
- **`JavaParserUtil.stoppingBuilder(builder, condition)`** (JavaParserUtil.kt:133) is the one method not ported.
  Nothing in intellij-community at decc943 calls it (the parsers call only `stoppingBuilder(builder, stopAt)`, from
  `braceMatchingBuilder` and `DeclarationParser.parse`), and its adapter would call the condition on every
  `tokenType` and `eof()` of every builder, where `SyntaxTreeBuilder` checks one offset. It was a `todo!` stub, a
  panic path in a crate that must never panic, and is removed; `java_parser_util.rs` says where it would go.

### Parser driver and chameleons (`psi-builder` `src/tree.rs`, `kotlin-syntax` `src/language.rs`)

`Parser` owns every buffer a parse needs:

- the frames (`Language::Frame`: Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl` with its `PsiBuilderImpl`, Java's
  `PsiBuilderImpl`), each a builder with all its buffers (lex types and starts, markers, free lists, production,
  entries, the join/newline/adapter stacks);
- the file's and the relexed `Lexed`;
- the tree being built.

`frames[..depth]` are the builders still binding; the ones past `depth` are finished and are reset in place
(`PsiBuilderImpl::reset`: cleared, marker 0 pushed) for the next file or chameleon, so no builder is moved or rebuilt
per chameleon: moving them (a builder is 584 bytes, with its buffers 992) through the language's parse and the frame
stack cost about 5.5 KB of copies per chameleon. Nothing is global or thread-local.

PSI builds blocks, lambdas, doc comments and markdown links as chameleons: each is reparsed in a fresh builder over
its own text. The port does the same, in place:

1. `bind` is a resumable state machine that stops at each chameleon.
2. The driver leaves the suspended frame where it is, parses the chameleon on the next frame (reusing the file's
   tokens when they cover it exactly, relexing otherwise), binds it, then resumes.

No native recursion is involved, so chameleon nesting takes heap, not stack, and node
ids come out strictly in preorder.

### Nesting limit (`builder::NESTING_LIMIT`)

Within one builder the parser is recursive descent, as the Java is: `(((…1)))` recursed once per level and a 12 KB file
of parentheses overflowed a 2 MB stack, which aborts the whole process. PSI has no defined result there (kotlinc
overflows too), so the port defines one, and keeps PSI's tree below it.

- **What is counted.** `PsiBuilderImpl::enter_nesting` counts a level at every recursive entry point: an expression
  (`parsePrefixExpression`, which every expression recursion passes), a type (`parseTypeRefContents`), a member
  declaration (`parseMemberDeclaration`), a type argument list (`tryParseTypeArgumentList`), an annotation and a
  parenthesized type. A chameleon's builder starts at 4 levels over where its block or lambda was collapsed
  (`MarkerData::depth`), so frames are bounded too; body-less trees keep that depth per unexpanded body for
  `parse_body`.
- **The limits.** 1000 levels; 32 nested type argument lists per builder, because `a<a<…` tries one at every `<` and
  rolls it back (a list that reaches 32 also makes every list starting before that token too deep, so each `<` costs
  constant time on average); 10 nested annotations and parenthesized types per builder, because a rolled-back attempt parses
  them and the parse after it parses them again (`@A(@A(…))`, `((…) -> A) -> A`), which doubles the work per level:
  measured on c796e73, 14 levels of `@A(` took 7.3 ms and 1000 did not finish. Stack per level, measured with a stack
  pointer probe and by bisecting the depth that overflows 1 MB without a limit: 127 B (unary minus) to 559 B
  (nested `object`), 382 B for parentheses; random mixes of 42 forms stayed below 320 B. 1000 levels of the worst
  form fit 640 KB, not 512 KB. The community files reach 62 levels, 5 type argument lists and 4 reparsed
  constructs.
- **Past a limit** the construct is an error element "Code is too deeply nested" over the rest of it: from the
  current token to the first token that closes a bracket opened before it (`(`, `[`, `{`, `${`, quotes), at least one
  token. The table of these closers is made once per builder when first needed, so the error costs constant time,
  also inside an attempt that is rolled back; the construct around it then closes normally. A chameleon past the limit
  is not parsed: its node gets one such error with its tokens as leaves.
- **Frames.** A suspended frame drops the tokens `bind` has passed once they are at least 4096 and half of what it
  holds (`drop_bound_tokens`, keeping the first token of any collapsed marker still ahead), so nested chameleons hold
  O(tokens) memory, not O(depth · tokens).

`tests/robustness.rs` parses 22 forms nested 100 000 deep on a 1 MB stack, and every golden with limits 1 to 13;
the gate runs the community files and the mutations with limit 3 (see Contract).

The limits are `Language` constants (`NESTING_LIMIT`, `TYPE_ARGUMENTS_LIMIT`, `REPARSED_LIMIT`), whose defaults are
Kotlin's. A construct parsed twice that reaches its limit also makes every such construct starting before that token
too deep (`reparsed_too_deep_before`, as for type argument lists): otherwise the constructs re-parsed after the
rollback of the one around them, each reaching the limit again from a shallower start, cost a polynomial of the
input (Java's `@A(f(@A(f(…` grew as n³ with a limit of 3); below the limits nothing changes.

### Java nesting (`java-syntax`; `Java::NESTING_LIMIT`)

The Java parsers recurse once per level as Kotlin's do: 20 000 nested parentheses in an initializer overflowed the
stack. They count on the same `psi_builder` machinery (`SyntaxTreeBuilder::enter_nesting`), and past a limit a
construct is the same error element over the rest of it (`java_parser_util::too_deeply_nested`, which returns the
error's marker as the construct the caller parsed).

- **What is counted.** Every recursion cycle passes one counted function: an expression
  (`tryParseWithPrecedenceAtMost`) and a unary operand (`parseUnary`'s operand of a prefix operator or a cast, the
  recursion that does not pass it); a statement (`parseStatement`); a declaration (`DeclarationParser.parse`, once
  the token may start one: members, local and anonymous classes, enum constant bodies); an array initializer (an
  annotation's recurses through `parseAnnotationValue` without an expression); a type (`parseTypeInfo`); a type
  argument list (`parseReferenceParameterList` at `<`, `Nesting::TypeArguments`); an annotation
  (`Nesting::Reparsed`); a pattern (`parseTypeOrRecordPattern`, record patterns); an inline doc tag (`parseDataItem` at
  `{@`). `ModuleParser` and `JShellParser` have no cycle of their own. A code block is a body and starts its frame at
  `NESTING_CHAMELEON` over where it was collapsed; a doc comment and its reference holders start at 0 with the parser's
  limit (`Chameleon::builder`), as a KDoc does: they hold no body, so a frame is bounded by its own tokens.
- **The limits.** 1200 levels: IntelliJ's own test `Parenth2.java` (in the community corpus and the goldens) nests
  1001 parenthesized expressions, 1010 levels; the next deepest community file reaches 174 (`NestedLambda.java`), the
  deepest production file 102. A level takes at most 652 bytes of stack in a release build (typed and parenthesized
  lambdas, enum constant bodies; measured by bisecting the depth that overflows 1 MB without a limit, over 45 forms:
  127 B for unary operators and casts, 461 B for parentheses, 588 B for calls), and the deepest forms at the limit
  need 772 KB (bisected stack size), so a parse fits 1 MB. 32 type argument lists (the community files reach 4). 5
  annotations (the community files reach 4): an annotation in an expression is parsed up to 4 times per parse of the
  one around it (as a type's annotations, as the expression's, in a cast and in a lambda's parameters, each rolled
  back), so `@A(f(@A(f(…))))` took 3 times the work per level and 16 levels 12.6 s; at 5, an annotation takes at most
  4^5 = 1024 times its own work, as Kotlin's 2^10.
- **Block ends in constant time.** `braceMatchingBuilder` (every class body) and `parseBlockLazy` (every code block)
  find the `}` that closes a block with a loop over the tokens, which nested class bodies in one builder and nested
  bodies across frames ran once per level: 16 000 nested classes took 2.3 s, anonymous classes 5.8 s.
  `PsiBuilderImpl::balanced_block_end` answers what that loop finds. Both callers ask right after consuming the `{`,
  so a builder over a slice of the file's tokens reads the `}` that closes it from the file's brace pairs (one stack
  pass over the file's tokens per file, `builder::brace_pairs`, shared by every such builder; not when a brace of
  the builder was remapped). Otherwise it counts the raw tokens while the builder has
  counted fewer than it has tokens, then makes a table of every token's answer in one pass (the stack pass of the
  too-deep closers); a body deeper than `SHARED_BLOCK_DEPTH` (64) takes the file's table, made once per file
  (`Language::BLOCK_BRACES`), instead of counting its own tokens again in every frame. A remapped bracket makes the
  table stale up to it (Java's parsers remap no brace). The marker and rollback around the loop stay, so the builder's
  state after it is the Kotlin's; `parseBlockLazy` ends at the `}` or at the first token from the stopping offset on,
  as the loop does.
- **Markdown labels.** `parseMarkdownReferenceOrLink` scans from each `[` to the `]` that balances it, remapping the
  DOC_SPACEs on the way: `/// [[[…]]]` ran it at every `[` over the rest, 16 000 brackets 1.2 s. The `]` comes from
  `balanced_block_end_at` over the doc brackets, each DOC_SPACE is remapped once (`remap_label_spaces`: nested labels
  lie inside the one around them), and the bracket counts, only compared with each other and with 1, from whether a
  `[` is inside.

`tests/robustness.rs` (Java) parses 80 recursive forms nested 100 000 deep on a 1 MB stack (each under 110 ms; two
more at 10 000: see "Open items"), 44 chains and lists of 100 000 operators or elements (each under 70 ms), 40 random
mixes of 26 expression forms 3000 deep, random bytes and fragments, and every golden with limits 1 to 13 (lossless,
`parse_body` equal to the full parse's subtree); the gate runs `--check-bodies` on the Java corpora at the default
limit and at 3.

### C# (`csharp-syntax`)

The sources are Roslyn's at 35d9211b (AGENTS.md, "C#", has the table of each Rust file and the C# file it ports).

```
text ─ Lexer over SlidingTextWindow (the bytes as UTF-16 code units) ─> green tokens with trivia   (GreenArena)
     ─ SyntaxParser + LanguageParser (one Rust fn per C# method) ─> green nodes, bottom up         (GreenArena)
         ├─ each directive: DirectiveParser over the lexer in Directive mode
         ├─ each documentation comment: DocumentationCommentParser over the lexer in its XML modes
         └─ each hole of an interpolated string: a lexer and a LanguageParser over a window of the file
     ─ flatten: one walk, no recursion, nodes written by id into the tree's columns ─> Tree {psi_builder::Tree<CSharp>, diagnostics}
```

- **Not on `PsiBuilderImpl`.** Roslyn builds green nodes bottom up from children already made, copies them to add a
  diagnostic or skipped text (`WithDiagnosticsGreen`, `AddLeadingSkippedSyntax`, `SyntaxFirstTokenReplacer`), and
  hangs trivia on tokens. A marker production expresses none of that, so C# implements only `TreeLanguage` and fills
  `psi_builder`'s tree in preorder through `TreeBuilder` (see "Workspace"); the consumer API and the dump's layout
  are the same code as Kotlin's and Java's.
- **The green arena (`green.rs`).** A `Green` is a `u32`, a 2-bit tag (null, token, node, list) over a 30-bit index;
  a null slot is stored as `Green::NULL`, so slots are where Syntax.xml puts them (the generator flattens its
  `Choice` and `Sequence` fields as `TreeFlattening.FlattenChildren` does). A token record holds its kind and
  contextual kind, flags, its text, its leading and trailing trivia (runs of `Trivia` in a shared array) with their
  widths (44 bytes); a node or list record its kind, flags, slots (a run of a shared `Vec<Green>`) and full width (20
  bytes); a trivia record its kind and text, or the node of its structure (a directive, a documentation comment,
  skipped tokens) in the text's place (16 bytes). What few records have, an element's diagnostics, a token's value and
  a node type's boolean fields, is a `RareData` entry beside them, an index in the record (none: `NO_RARE`); a copy
  shares its original's entry, and a record made with other rare fields pushes a new one, so no entry changes. A token's text is a range of the
  source, `SyntaxFacts.GetText(kind)` for a token made by kind (`SyntaxFactory.Token`, a merged `>>`), or a string the
  arena keeps; `ValueText` is kept only where it is not the text (an escaped identifier, `@`, a directive's string,
  an XML entity, a cref's brace), and the flattened tree keeps those values beside it (`Tree::value_text`). Flags
  are Roslyn's (`IsNotMissing`, contains diagnostics, directives, skipped text, structured
  trivia), inherited from the children as `GreenNode`'s are.
- **Copies, never changes.** What Roslyn does by making a new green node (a diagnostic added, a token with other
  trivia, a first or last token replaced) appends a new record; the old one stays, garbage, until the file ends. So a
  node a reset point or a scan saved is valid after any rollback without reference counts, exactly as Roslyn's
  immutable nodes are. The garbage is bounded: the corpora make 1.04-1.07 records per node of the finished tree and
  10-17 bytes of records per source byte as the budget counts them ("C# memory"); past `green::budget_for` (256 bytes
  a source byte, at least 256 MiB) the parse is abandoned as on an exhausted stack ("C# nesting and work"). The budget
  charges a record the size it had when the budget was set (a token 52 bytes, a node 28, a trivia 24, whatever the
  layout now; `GreenArena::record_bytes`) and is checked where buffers that started empty at the file's start would
  grow (`BudgetMarks`), so where a parse is abandoned depends on the file alone, not on the layout or on the capacity
  kept from earlier files. The records themselves take 44, 20 and 16 bytes.
- **Buffers.** The arena, the token window, the trivia and list builders (`SyntaxListPool`), the directive store, the
  interpolated string and documentation comment parsers' buffers and the flattener's stacks belong to the `Parser`
  and are cleared, not freed, per file. The tree builder keeps nothing between files: the tree's columns are allocated
  once per file at the arena's estimate of its nodes (bounded by the text), written in place and handed to the tree shrunk to fit. A tree
  owns its text (`Arc<[u8]>`), nothing of the arena. A parse that leaves more than the retained bytes in them (64 MiB by default)
  shrinks them to half of that, each to the same share of its capacity ("Retained buffers").
- **Lexer.** `Lexer.cs` and its partial files ported method by method. Keywords are a perfect hash of the first eight
  bytes and the length into 1024 slots, built and checked at compile time, in place of `LexerCache`'s keyword cache.
  `QuickScanner` and `LexerCache`'s token map are ported: in the `Syntax` and `DebuggerSyntax` modes `Lex` first runs Roslyn's state machine
  over at most 42 bytes (whitespace and ends of line, an identifier, keyword, decimal integer or one-character
  punctuation, whitespace and one end of line) and looks the text up in a direct-mapped map of 4096 slots (288 KB,
  kept by the `Parser` from file to file); a hit makes the token `LexSyntaxToken` would make of that text, whatever
  precedes it, so the map changes no tree. Roslyn's map holds the shared green token; a token here records its source
  offset, so the map holds its kinds and widths and its trivia's (`QuickToken`) and a hit pushes the records anew
  (`GreenArena::quick_token`); tokens with diagnostics or values, or other trivia, are not kept. Each token keeps the lexer state after it (position, directives, the
  lexer-driven mode), what Roslyn keeps per blended token, so a parser that changes the lexer mode re-lexes from the
  last kept token. `DirectiveStack` is an index into immutable cons cells (`Directives.cs`' `ConsList`), shared by the
  lexer and every reset; `CompleteIf` and `CompleteRegion`, recursive in Roslyn, rebuild the cells in a loop, and
  `IsDefined` steps over the cells it would only walk through. Excluded `#if` sections are lexed in a loop.
- **UTF-16 semantics over bytes.** Roslyn's positions and widths are UTF-16 code units of a `SourceText`; the port
  reads the file's UTF-8 bytes as those code units (`SlidingTextWindow`): a character below U+0080 is 1 byte, below
  U+0800 2, any other BMP character 3, each one code unit; a supplementary character is two code units, a high and a
  low surrogate of 2 bytes each; a byte that starts no valid UTF-8 is one U+FFFD code unit of 1 byte. Widths are
  bytes, so every position, a sum of widths, is the byte offset the oracle maps Roslyn's UTF-16 position to (between
  the halves of a pair, the character's offset + 2), with no second pass and no UTF-16 copy of the text. What Roslyn
  asks of characters it asks of code units here: the character classes (`unicode.rs`, from the oracle runtime's
  `CharUnicodeInfo.GetUnicodeCategory`; a surrogate is in none), `PeekChar(n)`, a raw string's dedentation (which
  walks characters, so its errors are at Roslyn's characters). Two things are not sums of widths and are computed in
  UTF-16: `SyntaxFirstTokenReplacer` adds the width of skipped text to the offsets of the diagnostics of the node it
  copies although that node's start does not move, so such a diagnostic remembers the shift and where its text
  starts and `flatten` sums it in UTF-16 and maps it back (10 fuzz files differed before, 43519b4f); and a message
  may hold half of a supplementary character (a token's text, a code unit the lexer reports), which a Rust `String`
  cannot, so `Diagnostic::message` has U+FFFD there and `lone_surrogates` the code unit, which the dump prints as
  `\u{XXXX}` as the oracle prints .NET's string.
- **Nested parsers.** A directive is parsed by `DirectiveParser`, a `SyntaxParser` in Directive mode over the lexer
  that met the `#` (its token window is the lexer's buffer); a documentation comment, in Parse and Diagnose modes, by
  `DocumentationCommentParser` over the same lexer in the XML modes, into structured trivia (in None mode it is
  comment trivia). An interpolated string's holes are parsed as Roslyn parses them, by a temporary lexer and
  `LanguageParser` each, but over a window of the file (`Lexer::with_window`) rather than a string cut out of the
  token's text: every range is a file offset, the tokens have the source's text and Roslyn's red positions, and the
  temporary parsers borrow the arena and take their buffers from `InterpolatedStringBuffers` (one level of nesting
  each), so no hole allocates per token once warm.
- **Flatten (`flatten.rs`).** One walk in `ChildNodesAndTokens()` order: a list's elements are its parent's
  children, a null slot is skipped, a node is a composite over its full span, a token a leaf over its span (missing
  when missing), its trivia leaves before and after it in the token's parent, trailing on the trailing ones, a
  structured trivia a subtree whose root has the trivia's kind. The walk reads the arena's records directly and
  writes each node by its id into the tree's columns (`TreeBuilder::writer`), sized to
  `GreenArena::tree_node_estimate` (every token, trivia reference and node record, garbage and lists included), at
  most 2 nodes a source byte (the corpora's files have at most 1.41) and 16 for an abandoned parse, so that garbage
  up to the arena budget does not size and zero columns the tree never fills (`flatten::first_capacity`): the
  next id, the position and the run of slots it is in stay in registers, a token's trivia are walked in place, and
  only the runs it comes back to (a parent's remaining slots, a token's trivia after a structured one) go on its
  stack. A tree that holds a record twice can exceed the estimate (14 of the 31 267 runtime files, 61 of the 122 741
  mutations, no file of the other corpora); the walk counted its nodes, so it runs again into columns of that size. There is no buffer of build records and no copy of the
  finished tree. Positions are
  running sums of full widths, as the red tree computes them, so missing tokens, merged tokens and nested lexers'
  tokens land where Roslyn puts them. The tree's text is the source; where Roslyn's tree drops a token (once in the
  corpora, mutations and fuzz: `ParseConversionOperatorDeclaration` replacing a modifier by a skipped one) its
  positions no longer spell the source, and from the first leaf whose text differs the tree's text is the green
  tree's own (`ToFullString()`), so every element shows its text at its red position as Roslyn's does. The same walk
  copies the value of each token the arena kept one for into a side table of the tree, `(leaf id, end)` pairs over
  one string in id order, which `Tree::value_text` searches; every other token's value is its text, or one the port
  does not keep (a literal's).
- **Trivia ownership bits.** `kinds` has two bits for C#, `TRAILING` (0x2000) and `MISSING` (0x1000), and 12 bits of
  kind index (574 C# kinds: 252 token and trivia kinds below `NODE_BASE`, 322 node kinds). Which token a trivia leaf
  belongs to cannot be read off positions: the end of a line does not end trailing trivia in a directive, skipped
  tokens become trivia of the token before or after them, a missing token can have trivia of its own. `is_trailing(id)`
  says it: a trailing leaf (or structured trivia root) belongs to the nearest token before it, any other trivia to
  the nearest token after it. Kotlin and Java never set either bit; the split changed none of their 25 counters
  (30d40a2f).
- **Diagnostics.** Each green element keeps its `SyntaxDiagnosticInfo`s: a code, its arguments, an offset from the
  element's start (not its full start) and a width. `flatten` collects them in `SyntaxTreeDiagnosticEnumerator`'s
  order (a node's before its children's, a token's after its leading trivia's), each at its element's start plus its
  offset, clamped to the tree's length, and formats each message as `DiagnosticInfo.GetMessage(InvariantCulture)`
  does (`diagnostic.rs`: composite formatting of the English format, the arguments as .NET formats the objects
  Roslyn passes: token texts, kinds by name, feature names, versions, embedded infos, an XML error inside
  `WRN_XMLParseError`). `Tree::diagnostics()` has them all, in that order with their codes and byte spans; the psi
  tree's error table has the first message of each node, which `errors()` and `error_message` read.
- **Versions and entries.** `FileKind { version, entry, doc_mode }`: the feature checks (`CheckFeatureAvailability`,
  `IsFeatureEnabled`) read `version`; on the four corpora every Preview tree equals the Latest one (the oracle's
  Preview dumps are all hard links), so Preview's own paths run on the goldens, mutations and fuzz. The entries
  besides `CompilationUnit` are `SyntaxFactory.Parse*` for Roslyn's tests (hidden from the docs); `Entry::Name`
  parses at the defaults as `ParseName` takes no options; `Entry::Tokens` is `ParseTokens`, the lexer alone.
- **Oracle.** `tools/csharp-oracle` runs Roslyn itself and walks the red tree it returns (`ChildNodesAndTokens()` with
  each token's trivia, the order of Roslyn's own `ParsingTests.EnumerateNodes`): the dump is not imitated. Its
  goldens are the inputs of Roslyn's parsing and lexing tests, found and constant-folded by Roslyn compiling those
  tests (7051 of 7214 constant inputs, with each test's version, entry and documentation mode); a difference on a
  corpus is reduced to `crates/csharp-syntax/tests/oracle/`.

### C# nesting and work (`csharp-syntax`; `parser::MAX_RECURSION_DEPTH`, `green::work_budget_for`)

Roslyn bounds its parser by its thread's stack: `StackGuard.EnsureSufficientExecutionStack(_recursionDepth)` at
namespaces, members, statements and sub-expressions throws past a fixed reserve, and `ParseWithStackGuard` then turns
the whole text into one skipped `BadToken` with `ERR_InsufficientStack` (CS8078, "An expression is too long or
complex to compile") at the lexer's position. The port has no stack to measure, so a count replaces it, and the same
failure is its only answer to an input it will not parse.

- **What is counted.** `_recursionDepth` where Roslyn counts it, and the cycles it leaves unguarded with the same
  counter: `ParseType` and `ScanType`, initializers, query bodies, designations, negated patterns. A local function's
  body adds 2 more (`LOCAL_FUNCTION_BODY_DEPTH`), a hole of an interpolated string 16
  (`INTERPOLATION_RECURSION_DEPTH`). `a?.b?.c…` (`parseWhenNotNull` calls `TryParseConditionalAccessExpression` for
  each `?`, unguarded) is a loop over a stack of the pending `primary ?` pairs, built innermost first.
- **The limit.** 1000 levels. A level takes at most 760 bytes of stack in a release build (129 for a unary operator,
  418 for parentheses, 687 for a type argument list, 757 for a namespace: a stack pointer probe over 150 forms 900 deep
  on 93d30b1a); nested local functions took 1.9 KB a level and overflowed 1 MB at 1000 levels, hence their weight.
  The corpora reach 678 (dotnet/runtime's `hugeexpr1.cs`), the deepest other files 118 (aspnetcore), 50 (roslyn), 45
  (Avalonia).
- **Against Roslyn on its 64 MB stack** (the oracle, `--stack-mb 64`, the 153 forms of `tests/robustness.rs`): at 300
  levels 142 forms are Roslyn's tree byte for byte; the others are the four interpolated string forms (a hole counts
  16, so about 60 nested holes reach the limit) and six forms of the lexer (doc comment elements and crefs, directive
  expressions: `MAX_NESTED_SCAN_DEPTH`, 128). At 900 levels 121 are equal: 25 more reach the port's limit because a
  level counts more than once (initializers, lambda blocks, queries, `with`, slice patterns, attributes, local
  functions), where Roslyn's stack still holds. At 100 000 levels every form compared fails in both with the same
  tree, CS8078 at the same offset (26 forms so far; Roslyn needs minutes for others); Roslyn's process itself dies of a
  stack overflow on 100 000 nested array initializers (`ParseArrayInitializer` and `ParseVariableInitializer`,
  unguarded, 82 249 frames in 64 MB), on 100 000 nested interpolated strings (its lexer's
  `ScanInterpolatedStringLiteralHoleBalancedText`, 58 263 frames) and on 100 000 nested patterns (`ParsePattern`,
  `ParsePatternContinued`, `ParsePrimaryPattern`, 68 765 frames).
- **Work.** Speculation (a reset point, a scan or a trial parse, a rollback) repeats the work of what it covers, and
  nested it multiplies: `[[[…` parses an attribute list over the rest at every level (10.8 s for 100 000 levels),
  `a?[a?[…` parses the rest twice at every level (1.3 s for 400; Roslyn did not finish 900 in 25 minutes). Every token
  a parser consumes counts against `work_budget_for(len)`, 16 tokens a byte and at least 2^20, again after each reset
  (`SyntaxParser::charge_work`); past it the parse fails as an exhausted stack does. The corpora consume 0.14-0.27
  tokens a byte, at most 1.0 in a file of a kilobyte or more, and 250 in a 16-byte mutation; the gate proves no corpus,
  mutation or fuzz tree reaches the budget. A chain of 17 `?[0]` reaches it (20 reached the depth limit before);
  Roslyn parses 16 in 0.38 s, 20 in 0.54 s, 24 in 4.0 s.
- **Abandoning.** Past a bound every token a parser sees, current or peeked, is the end of its lexer's input
  (`SyntaxParser::abandon`), so its loops end and it returns, as Roslyn's unwind on the exception: an end of file, and
  for the documentation comment parser the end of the comment, since its lexer gives no end of file and
  `SkipBadTokens` in an end tag stops only at the comment's end. An end of file there was eaten without end, the list
  growing until memory ran out (a budget sweep over runtime files found it, 2026-11-03; `memory.rs` keeps the case).
- **Linear where Roslyn is not, with Roslyn's tree.** A finished tuple type scan that starts with `(` is replayed from a
  memo keyed by its token and context (`scan_tuple_type`): nested parentheses rescanned the tuples inside each level,
  21.7 s for `hugeexpr1.cs`, now 1.1 s. A stray token after a member adds itself to the trailing trivia of the
  member's last token, which Roslyn copies each time (n `}` after a class: 3.7 s for 100 000); a list of 8 or more grows
  in place into room of twice its length (`GreenArena::append_trailing_trivia`). `IsDefined` walked every directive
  of the stack, so excluded `#if` nested n deep took n² (3.4 s for 100 000); its cells skip to the next `#define`,
  `#undef`, `#elif` or `#else` and to the next `#if`.
- **Known deviations.** 100 000 `a < a < …` fail as too deep (`ScanType` recurses at each `<`; Roslyn's does too,
  without a guard); the 17 `?[` above; the lexer's `MAX_NESTED_SCAN_DEPTH`.

`tests/robustness.rs` (C#) parses 153 recursive forms nested 100 000 deep on a 1 MB stack (each under 220 ms), 101
chains and lists of 100 000 (each under 120 ms), 20 speculative forms at 250, 1000 and 100 000 levels (each under
225 ms), 40 random mixes of 38 forms 3000 deep, the lexer's deep forms, and random bytes and fragments at every entry,
language version and documentation mode; each tree is lossless, in preorder, and the same on a second parse.

### C# deviations

Every place the port does not do what Roslyn does, each named in a comment at its place ("not in Roslyn"). None
changes a tree of the gate's inputs; the first three apply only past limits no corpus file comes near.

- **Stack, work and memory bounds** ("C# nesting and work"): a depth counter of 1000 in place of the stack; the
  recursions Roslyn leaves unguarded counted on it (`ParseType`, `ScanType` through type argument lists, tuples and
  function pointers, initializers, query bodies, designations, negated patterns); a local function's body as 3
  levels, a hole of an interpolated string as 16; the work budget and the arena budget. Past any of them the tree is
  Roslyn's for an exhausted stack. Known cases where Roslyn would still parse: `?[` chains of 17 or more, 100 000
  `a < a < …`, and the forms at 300-900 levels that count a level more than once.
- **The lexer's and the documentation comment parser's recursion** stops at `MAX_NESTED_SCAN_DEPTH` (128) where
  Roslyn has no guard: a hole of an interpolated string inside another's and a bracket inside a hole are scanned as
  plain content past it, a directive expression's operand is a missing identifier with `ERR_InvalidPreprocExpr` and
  the rest of the line skipped, an XML element is not started (the rest of the comment is text), a cref name takes no
  type argument list, `extension` is a plain member name. Roslyn's own process overflows its 64 MB stack on 100 000
  nested interpolated strings.
- **Loops for recursions with the same nodes**: `a?.b?.…` (a stack of the pending `primary ?` pairs),
  `ConvertExpressionToType`/`ConvertTypeToExpression` over a name chain, `CompleteIf`/`CompleteRegion`, excluded
  `#if` sections.
- **Linear algorithms with Roslyn's decisions**: the replayed tuple type scans, trailing trivia grown in place,
  `IsDefined` stepping over cells, a run of `@` scanned once (`TryScanAtStringToken`).
- **Values not kept**: the `ValueText` of numeric literals (in directives too), of character literals, of string
  literals outside directives and of interpolated and raw string texts (no tree shows them; `Tree::value_text` is
  `None` there, a missing token's empty text aside); `MakeInterpolatedStringTextToken` lexes a text as a string literal only when that could report a diagnostic
  (an escape or a line break in a regular text, a quote in a verbatim one).
- **Where Roslyn throws `ExceptionUtilities.UnexpectedValue`** on a value no input reaches (a designation kind in
  `ParseForEachStatement`, the precedence of a kind `GetPrecedence` does not list, a lexer mode no parser asks), the
  port takes a plain path (the `ForEachVariableStatement` form, `Primary`, an end of file) instead of a panic.
- **Text Roslyn never sees**: a byte that starts no valid UTF-8 is one U+FFFD code unit of 1 byte (the oracle dumps
  only valid UTF-8); a lone surrogate of a message is U+FFFD with `Diagnostic::lone_surrogates` beside it; a tree
  whose token Roslyn dropped takes its text from the green tree past the drop.
- **Not ported**: incremental parsing (`Blender`, `BlendedNode`, the old tree), `LexerCache`'s trivia map and keyword
  cache (its token map and `QuickScanner` are ported: "Lexer"), the node cache, scripts, preprocessor symbols from
  options, `Features`.

### Tree (`psi-builder` `src/tree.rs`)

A tree is three columns over the node ids, in preorder:

| column | type | holds |
|---|---|---|
| `kinds` | `u16` | `ElementType::index` (12 bits: tokens below `NODE_BASE`, composite types from it), `LEAF` bit, `UNEXPANDED` bit, `TRAILING` and `MISSING` bits (Roslyn's trailing trivia and missing tokens; Kotlin and Java never set them) |
| `ranges` | `[u32; 2]` | start and end byte |
| `links` | `[u32; 2]` | parent and subtree end |

That is 18 bytes a node. The tree also holds the text (`Arc<[u8]>`) and an `errors` table sorted by id, which is
allocated only when the file has errors.

- **Navigation.** The subtree of `id` is `id..subtree_end`, the first child is `id + 1` when the subtree is not empty,
  and the next sibling is `subtree_end` while that is inside the parent's subtree. `last_child` walks the children.
- **Building.** Kotlin's and Java's tree is built in the `Parser`'s buffer, one record per node (one push), and copied
  into exactly sized columns; C#'s is written into its columns in place (`TreeBuilder`) and shrunk to fit. Either way
  there is no slack.
- **Leaves.** Whitespace and comments are leaves wherever PSI's binders put them. No string is stored per leaf: a
  leaf's text is its range.

### Consumer API

AGENTS.md, "The consumer contract", is the normative list. In short:

- `Parser::new()` once per thread, then `parse(text, FileKind)` or `parse_with_options` for each file. `Parser` is
  `Send`; `Tree` is `Send + Sync` and has no lifetime. A const assertion checks both.
- `descendants(id)` is an id range and `preorder(id)` a linear scan with depth, so "every `CALL_EXPRESSION` under X"
  reads one column.
- `leaves()` gives `(TokenKind, Range)` in document order. It is the only exact source for highlighting: soft
  keywords are remapped, and `?.`/`?:`/`!!` joined, by the parser, not the lexer.
- `ParseOptions { expand_bodies: false }` leaves each block and lambda as a childless node marked `is_unexpanded`. Doc
  comments are still expanded. `parse_body(id)` returns that body's exact subtree as its own `Tree`; the gate checks
  this on every community body.
- A `Parser` keeps its buffers from file to file up to its retained bytes (`set_retained_bytes`,
  `buffers::RETAINED_BYTES` = 64 MiB by default): a parse that leaves more shrinks them to half of that. `heap_bytes()`
  is what they hold, `shrink_to(bytes)` shrinks them on demand (the same for the three languages, "Retained buffers").
- C#: contextual keywords are `IdentifierToken` leaves unless the parser converts them (`partial class`, `async`,
  `record`, `where` clauses: their keyword kinds), as in Roslyn's tree (`tests/contract.rs`).
- C#: `csharp_syntax::Tree` derefs to the same `psi_builder::Tree` and adds `diagnostics()` (codes, byte spans,
  messages, in Roslyn's order) and `value_text(id)` (`SyntaxToken.ValueText`: `@class` is `class`, `\u0061` is `a`;
  "Value texts"). Trivia are leaves, or subtrees when structured, beside their token, `is_trailing`
  telling which token they belong to; an inserted token is an empty leaf with `is_missing`. There is no body-less
  parsing: Roslyn parses no block on its own.
- Kotlin: `kotlin_syntax::psi` answers what Kotlin's PSI getters answer (`hasModifier`, `getName`,
  `getTypeElement`, `unwrapNullability`, `isLocal`, ...), so accio and Branch Deck stop reimplementing them on raw
  kinds, where their walks had drifted (`Int??` peeled one level, `` `JvmName` `` with its backticks, `(@A Foo)?`
  read as its modifier list). Free functions over `&Tree` and `NodeId`, not typed wrappers: the consumers hold
  `NodeId`s already, and a wrapper per class would add a conversion at every call without catching a mistake the
  kind dispatch does not (a call on a node of the wrong class answers `None`, as the PSI method of a class that lacks
  it). `tools/kotlin-psi-getters/GetterOracle.java` checks them method by method against the live PSI.
- Java: `java_syntax::psi` answers the getters of IntelliJ's PSI classes (a field's type element across `int a,
  b[];`, `hasModifierProperty` with the implicit modifiers, a reference's qualifier and `getKindEnum`, ...): a module
  per class, a function per method, ported from `java/java-psi-impl` at the parser's revision and checked against
  the real PSI on every node ("Java PSI getters" below).

## Decisions

| decision | why | evidence |
|---|---|---|
| A faithful port, one function per Java method, MarkerProduction semantics | Byte equality with PSI on every input, including error recovery; a diff against the Java reads line by line | gate: 0 of 55 501 community files differ, and 0 of 3000 for each parse-fuzz corpus |
| Speed from representation, allocation and layout, never from parser decisions | Equality stays provable by the gate and the counters | all 25 counters identical across batches 2 and 3; in batch 1 only the cursor-call counts (token cache) and the lazy-block counts (brace skip) moved, as intended |
| Chameleons reparsed exactly as PSI does, not parsed eagerly in the file builder | Not equivalent: a fresh builder resets the newline and join stacks, the edge binders and the error dedup at the chameleon boundary | `reusable_tokens`' DANGLING_NEWLINE case; the brace skip removes the repeated work instead |
| In-place expansion over a heap frame stack | Preorder ids (1.03M preorder breaks under the old LIFO worklist) without native recursion | walk 3.62 → 2.23 ns/node |
| The production as a linked list over marker nodes, made into an array for binding | `precede` and the other middle edits in constant time with the array's entry order, so the same tree; a gap buffer or chunks would still need the lexeme search | gate unchanged; 100 000-operator chains 14-45 ms (`+`: 4.67 s → 22 ms); parse 72.7 → 74.2 MB/s |
| Java's nesting on the same counter, with its own limits (`Language` constants) | One mechanism and one error for both languages; Java's stack per level (652 B) and `Parenth2.java` (1010 levels) want 1200, its annotations (parsed up to 4 times per level) 5 | gate unchanged, now with Java's `--check-bodies` at limits 1200 and 3; 80 forms nested 100 000 deep on 1 MB |
| Block ends counted, then tabulated, the table shared by deep frames | `braceMatchingBuilder` and `parseBlockLazy` scanned to the `}` once per level; a table made by every builder cost that again in every frame of a deep chain | 16 000 nested classes 2.3 s → 1.9 ms; 100 000 nested `try` with a table per builder 515 ms (92 % making tables), counted then shared 38 ms |
| A nesting counter that degrades to an error element, not a bigger stack or `stacker` | Deterministic on every platform and build, bounded memory, no `unsafe`; the dump is PSI's wherever PSI itself parses | gate unchanged; 100 000 levels of 22 forms on 1 MB in at most 110 ms; parse 74.4 → 75.0 MB/s (noise) |
| The too-deep error over the rest of the construct, in constant time | One token at a time cost `2^10` times the rest of the file inside doubled annotations; a scan to the closer cost O(n) per rolled-back `<` attempt | `@A(` × 100 000: 4.2 ms |
| A per-thread `Parser`, no thread_local | Allocation churn dominated multi-threaded scaling | allocations per pass 10.7M → 179 779; 16 threads 684 → 917 MB/s (f3aa172 → a99fb45) |
| Copy the tree out exactly sized, instead of reserving `tokens*15/8+16` | Same CPU time, less memory | user CPU 3.12 vs 3.13 s; 11.32 vs 12.70 MB of tree per source MB |
| Pair columns `ranges`/`links`, not five `u32` columns | Same 18 B/node, 3 pushes instead of 5 | parse 70.0 → 71.5 MB/s (9 of 9 rounds), linear walk 1.68 → 1.59 ns/node |
| `psi-builder` generic over a `Language` trait, monomorphized; not `dyn`, not language-neutral numeric ids | The Java port reuses the builder, tree and driver; Kotlin keeps its enums and every parser call site (`kotlin_syntax` aliases the generic types), and the compiled Kotlin parse is the one it was | gate unchanged, all 25 counters identical; 1 thread B/A 0.985 before forcing `skip_whitespace` inline and leaving `getJoinedTokenType` outlined, after it paired-round medians 0.991 and 0.997 at load 6-14 (see the split's measurement) |
| No `unsafe`, no C toolchain, no large dependency tree | Consumers build it on every platform; any byte sequence must parse | `unsafe_code = "forbid"`; the only dependency is the optional `stats_alloc` |
| C#: Roslyn's own lexer and parsers ported method by method, the oracle Roslyn itself | accio needs Roslyn's tree and its diagnostics' messages; a faithful port reproduces error recovery and speculation, which no grammar rewrite would | 7051 of 7051 goldens; 0 differing dumps on the four corpora at Latest and Preview, the 122 741 mutations at three versions and the 20 000 fuzz inputs at two (43519b4f, gated since 174ecb80) |
| C#: a green arena flattened into `psi_builder`'s tree, not Roslyn's parser on `PsiBuilderImpl` | Roslyn builds bottom up, copies nodes and hangs trivia on tokens; flattening once keeps one consumer API for the three languages | the `TreeLanguage`/`TreeBuilder` split moved none of the 25 counters, `scripts/ab.sh` Kotlin B/A 1.017, Java 1.047 at load 16-20 (noise; 30d40a2f); flatten 13.6 % of a C# parse ("C# parse throughput and robustness") |
| C#: records never change, a copy appends, garbage kept to the file's end | Roslyn's copy semantics exactly: a saved node stays valid after any reset, with no reference counts or collector | 1.04-1.07 records per tree node, 10-17 bytes of records per source byte as the budget counts them on the corpora ("C# memory") |
| C#: widths in bytes over a UTF-16 reading of the text | positions are the oracle's byte offsets with no second pass or UTF-16 copy; only the one sum Roslyn mixes is redone in UTF-16 | 0 differing lexer lines on every corpus since 7d7af22e; the last 10 fuzz differences were that sum (43519b4f) |
| C#: TRAILING and MISSING bits in the shared `kinds` column | the token a trivia belongs to and an inserted token cannot be read off positions; two bits cost nothing per node | Kotlin and Java counters unchanged (30d40a2f); 12 bits of kind index hold C#'s 574 kinds |
| C#: a depth counter, a work budget and an arena budget, failing as Roslyn's exhausted stack does | deterministic on any stack and machine, with the answer Roslyn itself gives at 100 000 levels; time and memory linear in the input | 153 forms 100 000 deep on 1 MB each under 220 ms; 26 forms compared at 100 000 levels equal Roslyn's tree (43ed5b13); no corpus tree reaches a bound (the gate) |
| Buffers kept up to a bound; past it each shrunk to the same share of its capacity, to half the bound | A parser per thread for a process's lifetime kept its largest file's buffers (1.24 GB after one 24 MB C# file); freeing them all costs every next file its growth, a proportional share costs nothing to the files after it, and the half left free keeps files of other proportions from shrinking again at once | the four C# corpora keep 14.8-58.2 MB instead of 14.8-1245 MB, 16 shrinks in 62 615 files; the files after a shrink parse as fast as without one; freeing after every file costs 8-14 % ("Retained buffers") |
| C#: an interpolated string's holes lexed and parsed over windows of the file | Roslyn cuts strings out of the token's text; windows give the source's texts and Roslyn's positions with no allocation per token | the interpolation goldens, the corpora and the fuzz at 0 differences (43519b4f) |

### Libraries

A maintained crate is preferred to reinventing it when it is measurably as fast or faster and the output stays
byte-identical. None qualified. The measurements are from the survey prototypes and batch 2, on the community corpus:

| crate | verdict | evidence |
|---|---|---|
| phf 0.14 | rejected | `phf_map!` 47.5 MB/s against 47.7 for a byte `match` (base 41.0); it adds a build-time macro crate |
| simdutf8 0.1.5 | rejected | +0.6 %, noise; not validating at all (`get_token_bytes`) gave +4.8 %; whole-file validation is under 0.25 % of a parse |
| memchr 2.8.3 | deferred | table plus memchr against table alone: lexer only 608 vs 595 MB/s (+2.2 %), end to end +1.6 %, below noise; `memchr3` cannot take the 4 string-part needles |
| logos 0.16.1 | rejected | cannot `yypushback(n)` (9 rules), has no state stack with `lBraceCount`, and its Unicode classes differ from JFlex's Java tables; a simplified ceiling grammar was about 2 % faster than ours on the lexer, which is under 10 % of a parse |
| rowan 0.17, cstree 0.14, biome_rowan 0.7 | rejected | building their green trees on top of ours cost +44 % (rowan, per-file cache), +60 % (rowan, shared cache) and +70 % (cstree) of a whole parse; walks 2.9× (rowan) and 4.5× (cstree) slower; rowan tokens are `&str` (no BAD_CHARACTER bytes) and its `SyntaxNode` is `!Send` |
| bumpalo, typed-arena | rejected | lifetime-bound references conflict with an owned `Send + Sync` tree |
| id-arena, la-arena | rejected | the same as `Vec` plus `u32` ids |
| smallvec | rejected | reusing buffers in the `Parser` removes the same allocations |
| lasso, rustc-hash, foldhash | rejected | no crate has a hash map, no per-leaf `String`; interning is the consumer's choice |
| bitflags, fixedbitset | rejected | `TokenSet { [u64; 8] }` with const constructors covers tokens and node types; the `at_set` cost was the keyword loop and `from_utf8` |
| mimalloc 0.1.52 | consumer-side only | needs C. Before buffer reuse: +12 % at 8 threads, +15 % at 16. After it (a99fb45): +3.8 % at 8, +3.1 % at 16 (load 8-15). On the inline hints commit: no change in instructions, wall within noise at 1 and 16 threads ("Build and toolchain") |
| stats_alloc 0.1.10 | kept, optional | the `alloc-stats` feature, profiling only |
| tree-sitter-c-sharp 0.23.5 | not a dependency: the parser the C# port replaces in accio | 5.3-5.8× slower than the port on one thread, 5.4-9.2× at 16 threads; an error in 927 of the 17 075 roslyn files, where Roslyn reports one in 41 ("C# parse throughput and robustness") |

### Build and toolchain

psi-syntax is a library: its consumers' release profiles and flags decide how it is compiled, so the source carries
the inlining a profile finds (see "Pipeline") and the rest is guidance. psi-syntax's own release profile (fat LTO,
one codegen unit, unwinding; in accio the `psi` profile) is the one the measurements use. Per parse round, one thread, instructions of the parsing
thread, on the inline hints commit as first measured ("Inlining as a profile does it"; the flags were measured on
9b8c26f; on the integrated 7ce233e, `scripts/pgo.sh` with the goldens takes Kotlin 35.40 → 31.07 G and Java 43.48 →
37.7 G, see "The radical batch integrated"):

| build | Kotlin | Java | verdict |
|---|---|---|---|
| plain release | 52.13 G | 68.92 G | the baseline |
| PGO, trained on other real code (Exposed's 768 `.kt`, commons-lang's and gson's 793 `.java`) | 44.73 G (−14 %) | 57.05 G (−17 %) | for consumers' binaries: `scripts/pgo.sh` |
| PGO, trained on the vendored goldens (the script's default) | 45.31 G (−13 %) | 59.92 G (−13 %) | |
| PGO, trained on the benchmark corpus itself | 45.19 G | 57.62 G | no better than other real code |
| `-Cllvm-args=-inline-threshold=1000` | 51.74 G (−0.8 %) | 63.61 G (−7.7 %) | a consumer's choice; on master −3.7 % / −11.2 %, 500: −2.3 / −8.2 %, 100: +2.5 / +2.1 % |
| `panic = "abort"` | −1.6 % | −1.2 % | a consumer's choice (no unwinding to catch a panic, which the contract rules out) |
| `-Ctarget-cpu=native` (apple-m5; the target's default is apple-m1) | −1.0 % | −1.1 % | not portable, not worth it |
| mimalloc 0.1.52 | 52.07 G (0) | 68.97 G (0) | see below |

PGO in wall time, `scripts/ab.sh`, master against the hints commit built by `scripts/pgo.sh` (goldens), load 8-16:
Kotlin 73.1 → 81.3 MB/s (5 rounds, ×1.11), Java 60.6 → 76.6 MB/s (5 rounds, ×1.26), Kotlin 16 threads 866 → 898 MB/s
(3 rounds, ×1.04, the machine's other load shares those cores); core cycles, alternating, Kotlin 10.60 → 8.63 G and
Java 14.19 → 10.55 G (the held-out profile). Its dumps of the community files (Kotlin and Java), both parse fuzz
seeds and the Java golden mutations are byte-identical to the oracles. Applied to one crate at a time (a `RUSTC_WRAPPER`
adding `-Cprofile-use` by crate name), the profile's gain is almost all in the language crate, where the parsers,
the lexers and the builder's instances for that language are compiled: Kotlin 54.21 → 46.26 G with it on
`kotlin-syntax` alone, Java 75.42 → 58.14 G on `java-syntax` alone, −2.5 % on `ksdump` (the bind's instances), under 1 % on
`psi-builder`. Within the language crate (a profile filtered to some functions), the parsers' functions take −4.5 G
Kotlin and −8.4 G Java, the builder's −3.0 / −6.8 G, the lexers' −1.0 / −7.1 G; the hints recover the lexers' part and
some of the builder's. The rest is branch weights and code layout in the parser ports, thousands of call sites.

A profile is tied to rustc's LLVM version and to the code it was trained on (functions whose code changed lose their
profile, with a warning, not a wrong result), and its function names carry Cargo's crate hashes, so a committed
`.profdata` would not match a consumer's build; `scripts/pgo.sh` is the recipe instead. A consumer runs it, or the same
three steps in its own build: `RUSTFLAGS=-Cprofile-generate=DIR cargo build --release`, run the binary over a few MB
of Kotlin and Java, `llvm-profdata merge -o merged.profdata DIR`, `RUSTFLAGS=-Cprofile-use=merged.profdata cargo build
--release`. `llvm-profdata` must be of rustc's LLVM major version (rustup's `llvm-tools`, or Homebrew's `llvm@23` for
rustc 1.99); the instrumented run warns about value-profiling counters running out, which is harmless.

mimalloc, re-measured on the hints commit: no change in instructions (about 5 allocations per file), and wall time
within the noise at load 15-18 (1 thread B/A 1.078, 16 threads Kotlin 1.000, Java 1.057). The parse allocates too
little for the allocator to matter; it stays the consumer's choice. BOLT is not available: Homebrew's LLVM 23 ships no
`llvm-bolt` on macOS, and BOLT's Mach-O support is experimental.

## Measurements

The machine is an Apple M5 Max shared with other sessions. Every comparison alternates its binaries. A one-thread run
reports the summed parse time; a multi-threaded run reports wall time. The corpus is the community list (55 501 files,
172.91 MB).

### C# parse speed, the six commits after 25dac59f, 2026-11-03

What accio (JetBrains/accio#143) runs: dotnet/runtime's files up to 4 MiB (31 261 files, 353.55 MB), `ksdump --bench
--lang csharp --doc none --retain 32`, one `Parser` per thread. On this Mac the instructions and cycles of a round are
`KS_INSTR=1 scripts/ab.sh`'s (a `--repeat 3` and a `--repeat 1` run under `/usr/bin/time -l`, their difference
halved). Other sessions shared it (load 13-60), so its cycles and MB/s moved by up to 1.5× between rounds and only
its instructions, steady within about 2 %, are evidence here; the speed is the Mac Studio's, below. A sampled profile
of 25dac59f had flattening and tree building at 28.9 % of the time, the lexer 27.4 %, `LanguageParser` 22.1 %, the
arena 10.2 %, the token window 7.1 %, `memmove` 3.6 % (most of it `push_node`).

Each step against the commit before it, both values from one alternating run of 3-5 rounds (medians; one thread but
the last). Each run measured its own pair at its own load, so a step's "before" is not the row above's "after" and
the product of the steps (-30.0 %) is not the stack's change:

| step | G instructions a round, before → after | change |
|---|---|---|
| `create_node` over `&[Green; N]` (no `memmove` per node) | 109.9 → 108.2 | -1.5 % |
| token kinds in the token window | 108.2 → 102.4 | -5.4 % |
| `TreeBuilder` writes the columns, no `take_exact` | 101.8 → 104.8 | +2.9 % (columns grow by doubling) |
| flatten writes the tree's columns by id, in place | 104.8 → 86.4 | -17.6 % |
| Roslyn's `QuickScanner` and `LexerCache` token map | 85.5 → 75.4 | -11.8 % |
| rare fields in a side table (records 52/28/24 → 44/20/16 bytes) | 76.0 → 76.3, 18 threads summed | +0.4 % |

The side table is kept for memory, its speed effect unresolved: a parser's buffers for a file over 100 KB went 25.2 →
20.9 bytes a source byte (median, each file from empty buffers; p90 41.2 → 33.9), while its +1.3 % per-thread MB/s
and -1.4 % cycles (18 threads, 5 rounds at load 13-17) are inside the noise: 8 pairs at load 37-52 later spread from
-12 % to +18 % per pair (median +2 %), with instructions 0.2-0.5 % higher in each.

The whole stack against 25dac59f, 5 alternating rounds, one thread: 110.3 → 75.6 G instructions (-31.5 %, 0.312 →
0.214 G a source MB), 20.4 → 14.8 G cycles; 18 threads, cycles of all threads 21.6 → 16.1 G (-25.5 %), instructions
110.2 → 76.2 G. A review then made the arena budget charge the records' 25dac59f sizes and check where buffers grown
from empty would grow, and bounded flatten's first columns (both amended into their steps): one thread, 5 rounds
against the stack before them, 75.2 → 75.9 G instructions (+1.0 %; the rounds spread 74.3-75.6 against 74.5-76.0), and
110.3 → 75.9 (-31.2 %, 0.312 → 0.215 G a source MB) against 25dac59f. The trees are byte-identical (`scripts/gate.sh`
on each commit; 2 560 535 099 tree bytes and 122 593 031 nodes of the same files at `--doc parse`). A parser's buffers
for a file over 100 KB went 32.8 → 20.9 bytes a source byte (median, each file from empty buffers): the tree builder
keeps no buffer and the records shrank; the quick scanner's map adds 288 KB a thread (and one per nesting level of
interpolated string holes).

Where the budget abandons a parse is 25dac59f's on 31 pathological files (2.2 MB each but one of 86 KB: `a?[0]` chains
of 3-16 links in 5-100 % of the lines, `a?[a?[…`, `[` repeated, nested tuples, generic-looking calls; each file alone,
so from fresh buffers): the same dump each, where the side table before the review parsed 3 of them in full (a chain
of 5 at 100 %, of 6 at 25 % and 50 %). A sweep of budgets from 16 KiB by 6 % over 24 runtime files of 5-150 KB and 7
pathological texts of 30-60 KB, each parse from fresh buffers, gives the same dumps and the same smallest budget that
keeps each file as 25dac59f does (with the end of comment of "C# nesting and work", "Abandoning", applied to both,
without which 25dac59f ran out of memory in the sweep). Before the bound flatten sized its columns to the arena's
records, garbage included: 2.2 MB of `[` peaked at 547 MB, 343 MB a commit earlier; with it 287 MB (25dac59f
326 MB), and none of the 31 files peaks higher than on 25dac59f.

On the Mac Studio (M1 Ultra, 16 P and 4 E cores; the stack before the review, 3be66ee8, built there from 25dac59f and the stack's
diff, its source digest checked; default retained bytes), per-thread parse MB/s, medians of the rounds; pass 1
alternated 25dac59f first, pass 2 the stack first:

| corpus, threads, `--doc` | pass 1 (load) | pass 2 (load) | both passes |
|---|---|---|---|
| runtime, 1, none | 53.4 → 82.5 (12-15) | 54.9 → 82.7 (6-7) | 54.3 → 82.6 (×1.52) |
| runtime, 1, default | 52.9 → 79.1 (8-10) | 52.9 → 79.1 (5) | 52.9 → 79.1 (×1.50) |
| runtime, 20, none | 42.7 → 62.6 (7) | 41.1 → 46.7 (13) | 42.0 → 55.4 |
| runtime, 20, default | 40.5 → 60.7 (7-8) | 30.5 → 44.2 (17) | 35.6 → 55.1 |
| roslyn, 20, none | 58.3 → 81.1 (6) | 54.2 → 75.2 (14-15) | 56.9 → 79.0 |

accio's bar, 50 MB/s a thread at 20 threads on runtime, is cleared on the quieter pass (62.6, rounds 62.5-64.5) and
missed at load 12-17 (46.7); a second quiet 20-thread pass is still owed. The Studio measured the stack as a whole:
the side table alone, and the map's 4096 slots against 2048 (77.6 against 77.9 MB/s at 18 threads on this Mac in a
run whose output was not kept, +0.4 % instructions), were not measured there.

### Java PSI getters (child of 18211728), 2026-11-03

`java_syntax::psi` ports the PSI getters both consumers reimplemented on raw kinds (accio's `extract/src/java`,
Branch Deck's `code-symbols/src/java`). `tools/java-oracle/JavaPsiOracle.java --getters` asks the real PSI (IntelliJ
2027.1 at the parser's revision) the same getters on every node and writes one line per answer; `ksdump --getters
--check` compares ours: 0 differing on the 1036 goldens (29 190 lines), the 19 116 mutations (622 726), the 20 000
lexer fuzz inputs (848 006) and the 68 260 community files (76 008 352). 10 012 answers are `!` (all an implicit
class's `getImplementsList()`, a light list IntelliJ fails to build there); ours is `None`. The oracle takes 51 s for
the four inputs at once (one JVM each), the check 2.8 s for the community files on every core, so the gate grows by
some 4 s. Two shapes the first comparison caught: an intersection or disjunction of one type element (`A |` with an
error) is that element's type (`createDisjunction` of one), and the field walk of a 30 000-name declaration is
quadratic when asked of every field (37.8 s), so a pass takes `field::declaration_heads` (0.1 s for the test's
inputs together).

A review against the sources then asked the oracle what the first lines left out (line format 2, `getters/2/`): a
type's deep component with its reference element or primitive name (the first lines named only the outer class), a
`DIAMOND_TYPE`'s type, a class's keyword, `isValueClass` and own fields, methods and inner classes, an argument
list's count, a module import's reference, and the name identifier and type element of every
`PsiNameIdentifierOwner` and `PsiVariable`. Three interface getters had missed a class: a deconstruction pattern
variable's type element is its pattern's (`R(int a) r`), a labeled statement's name identifier its label, a module's
its module reference; all three answered `None`. With them: 0 differing on the four inputs (81.9 million lines,
70 s for the oracle), on the JDK 26 sources (15 413 files) and on the 66 305 Java files of the monorepo outside
`community/` (149.4 million lines for the two; one generated 4.5 MB file IntelliJ fails to load).

### Value texts (child of "(csharp) Shrink a parser's buffers past its retained bytes"), 2026-11-02

`Tree::value_text` against Roslyn's `SyntaxToken.ValueText`: `tools/csharp-oracle/oracle.sh values --kept` prints
every token whose value is not its text and is one the port keeps (it leaves out the literals: numeric, character,
string outside directives, raw string, interpolated text), `ksdump --values` ours, compared line for line
(`ksdump --values --check`). The port's lines (identifiers, entities, cref braces and entities, directive strings
and names), all equal to Roslyn's; the gate compares the corpora, the mutations and the fuzz on every run, and
`tests/value_text.rs` the goldens (`tests/values/goldens.values.txt`):

| input | lines | of which directive strings |
|---|---|---|
| dotnet/roslyn (17 075 files) | 13 385 | 3 228 |
| dotnet/runtime (31 267) | 29 738 | 16 |
| dotnet/aspnetcore (10 188) | 5 046 | 368 |
| Avalonia (4 085) | 592 | 0 |
| the 7051 goldens at their entries, versions and modes | 587 | 121 |
| the 122 741 golden mutations | 14 591 | 766 |
| the 20 000 lexer fuzz inputs | 2 580 | 565 |

The fuzz found 7 wrong values first: Roslyn keeps a token's `StringValue` in its `TokenInfo`, the port in the lexer's
`value`, which a directive in the token's trailing trivia (`@c #r "a"`, a directive not first on its line) overwrote
with its own tokens' values before the token was made; `lex_syntax_token` now keeps it across the trailing trivia. No
tree changed (the gate). The table costs a branch per token in `flatten` and nothing in most files (no token has a
value: the table and its string allocate nothing): `scripts/ab.sh` against the commit before this entry, 7 rounds at
load 2, roslyn B/A 0.988 and aspnetcore 0.985 in wall time, while the instructions of a roslyn run (`/usr/bin/time -l`,
files read included) went 63.05 → 62.17 G (-1.4 %): code placement, within what a layout change moves.
`tests/value_text.rs` ports Roslyn's expectations (`LexicalTests.cs`' identifiers, verbatim keywords, keywords and
punctuation, multiple `@`; `NameParsingTests.cs`' formatting characters; `CrefLexerTests.cs`' and
`NameAttributeValueLexerTests.cs`' entities, escapes and braces; `PreprocessorTests.cs`' directive names and files) and
compares `tests/values/values.cs` and the goldens with the oracle's lines of them. Missing tokens (`#pragma checksum`
without its strings, `#line` without its number) have the empty text, a missing literal's included, as Roslyn's
`MissingTokenWithTrivia`; the oracle's lines cannot show it (a value equal to the text is no line), a test does.

### Retained buffers (child of 6e8c19af), 2026-11-02

`ksdump --bench` (one thread, one `Parser`, list order, load 1.4-2.8) with `--file-stats`' new `parser_grown` column:
what a parser's buffers hold when a parse ends (`Parser::grown_bytes`, before the retained bound applies). With
`--retain 0` every file starts from empty buffers, so the column is what each file needs alone:

| corpus | median file | p99 | the largest | files over 32 / 64 MiB |
|---|---|---|---|---|
| dotnet/roslyn | 0.08 MB | 4.52 MB | 97.1 MB (`NullableReferenceTypesTests.cs`, 6.59 MB) | 9 / 1 |
| dotnet/runtime | 0.09 MB | 4.72 MB | 1237.8 MB (`hugeexpr1.cs`, 24.12 MB; `HugeArray1.cs`, `HugeField1.cs`, `HugeField2.cs` of 10.0-10.7 MB 651.5 MB each) | 43 / 18 |
| dotnet/aspnetcore | 0.08 MB | 2.18 MB | 76.4 MB (`MatcherAzureBenchmarkBase.generated.cs`, 3.05 MB) | 1 / 1 |
| Avalonia | 0.08 MB | 1.76 MB | 14.4 MB (`UnicodeData.trie.cs`, 0.37 MB) | 0 / 0 |

Of the 62 615 files 20 need more than 64 MiB, the smallest of them 1.04 MB; a file over 100 KB needs 25 bytes a source
byte (median; p90 46, the most 92). The arena is 733 MB of `hugeexpr1.cs`'s 1238 ("C# memory"), the tree builder,
token window and lexer the rest. What a thread keeps after the corpus, without a bound (`--retain max`, as before)
and with the default (64 MiB, shrunk to 32 MiB when a parse passes it), and the throughput (`ksdump --bench`'s wall
time, parse and flatten) of one run of each, two alternating runs of each on runtime:

| corpus | kept without a bound | kept with the default | shrinks | MB/s without / with |
|---|---|---|---|---|
| dotnet/roslyn | 101.5 MB | 51.1 MB | 1 | 122.8 / 122.7 |
| dotnet/runtime | 1245.0 MB | 58.2 MB | 14 | 78.9, 77.5 / 76.8, 76.1 |
| dotnet/aspnetcore | 76.5 MB | 33.7 MB | 1 | 87.8 / 87.7 |
| Avalonia | 14.8 MB | 14.8 MB | 0 | 82.6 / 81.3 |

Shrinking the buffers to their share of half the bound keeps them in the proportions of the files that grew them.
Shrinking them to exactly the bound did not: every later file whose proportions differ (a documentation comment after
`hugeexpr1.cs`) grew some small buffer past it and shrank all of them again, 132 times on runtime. What a shrink costs,
on runtime in 4 alternating rounds of each (the first, a warm-up, dropped; the summed parse times of the other 3, the
shrink itself timed inside `parse`): the corpus 5.68-5.78 s without a bound, 5.77-5.86 s with it (+1-2 %); of that the
14 files that shrink 1.77-1.79 s, 1.87-1.88 s (+5.7 %, 7 ms a file: they grow again from 32 MiB, by doubling); the 100
files after each shrink 0.145-0.148 s either way, the other files 3.77-3.84 and 3.75-3.82 s. The 0.1 s of the 14 files
is the corpus's whole cost, the 1.8-2.7 % of the table's alternating runs; a single run's difference of a point or two
is noise of the size of Avalonia's, which never shrinks. Freeing everything after every file instead (`--retain 0`,
single runs, each against a run with the default made with it) cost roslyn 122.7 → 105.7 MB/s (-14 %), runtime 74.7 →
70.8, aspnetcore 87.7 → 81.5, Avalonia 81.3 → 79.3, Kotlin's community files 112.0 → 102.7 (-8 %) and Java's 120.6 →
110.9 (-8 %): growing the buffers again costs the files after a free, which is why a shrink keeps a share. A vector of
nested buffers (a Kotlin or Java parser's frames, C#'s nested interpolated string levels and list pools) counts its
share in elements, so it keeps its first element even when that share rounds down to none: a parser's first frame is the
file's builder, and a share under a quarter of the frames freed it whole (the 2.6 MB Kotlin file of `robustness.rs` kept
0.58 of a shrink's target, now all of it; `buffers::Shrink::nested`).

Kotlin and Java (`psi_builder::Parser`, the same mechanism over its tokens, frames, tree builder and tables): over
the community files a parser keeps 15.6 MB (Kotlin) and 37.8 MB (Java); the most one file needs is 8.6 MB
(`MetadataStorageImpl.kt`, 1.06 MB) and 30.9 MB (`java_after.java`, 0.44 MB); a file over 100 KB needs 14 and 18 bytes
a source byte. No community file reaches the bound: 112.0 / 111.6 MB/s (Kotlin) and 120.6 / 120.3 (Java) without and
with it. The tests (`memory.rs` for C#, `robustness.rs` for Kotlin and Java) shrink after a 2 MB file and compare every
later tree with a fresh parser's.

### C# memory (19b40696), 2026-11-02

A probe (a test that was not committed, reading `GreenArena::record_bytes`, `record_counts` and `heap_bytes` after
each file) parsed each corpus of tools/csharp-oracle in list order with one `Parser` on one thread, at Latest with
documentation comments parsed and the default budgets. Arena records count every record made, the garbage of copies
and rolled-back speculation included; the capacity is what the `Parser` keeps after the corpus, set by its largest
file:

| corpus (MB) | tree nodes | green tokens / nodes and lists / trivia | records per tree node | record bytes per source byte | most in a file of 1 KB or more | arena capacity kept |
|---|---|---|---|---|---|---|
| dotnet/roslyn (249.94) | 55 559 618 | 23.63M / 22.89M / 12.95M | 1.070 | 9.85 | 34.7 | 57.8 MB |
| dotnet/runtime (426.88) | 162 757 775 | 70.42M / 62.64M / 36.88M | 1.044 | 16.57 | 54.7 | 733.2 MB |
| dotnet/aspnetcore (62.04) | 20 660 580 | 8.85M / 8.32M / 4.73M | 1.060 | 14.67 | 28.2 | 50.0 MB |
| Avalonia (23.59) | 8 001 383 | 3.33M / 3.05M / 2.01M | 1.049 | 14.68 | 26.4 | 7.7 MB |

A tree node is a token, a trivia or a node, and a list's elements are its parent's children, so a garbage-free
parse would make about one record per tree node: the copies and speculation add 4-7 %. The budget
(`green::budget_for`, 256 bytes a source byte) is 4.7 times the most any file needs. The trees are 5.00-7.86 MB per
source MB ("C# parse throughput and robustness"). The 733 MB of capacity are kept from dotnet/runtime's
`hugeexpr1.cs` (24.12 MB of source; buffers grow by doubling, so capacity exceeds the records); a parser now shrinks
them past its retained bytes ("Retained buffers").

### The C# port, step by step (bb0fe20b → 174ecb80), 2026-11-02

Goldens passing of 7051 (`cargo test --release -p csharp-syntax --test goldens`) and differing files on the gate's
inputs, as each step's commit measured them:

| step | goldens | differences |
|---|---|---|
| kinds, node types, messages and the arena generated; flatten and the dump (bb0fe20b, d9994f56) | — | — |
| `SyntaxParser` and `LanguageParser` as 470 stubs (cd76bb18) | 2 | — |
| the lexer, directives and directive parser (7d7af22e) | 254 | lexer lines at `--doc none`: 0 on the four corpora, the mutations and the fuzz |
| the expression and pattern parsers (6b2e8472), statements (cdd344e0), documentation comments and interpolated strings (1adb25e9) | 564, 616, 1393 | lexer lines at `--doc parse` and `--doc diagnose` 0 (were 12 142, 13 832, 7 482, 3 410, 2 078 and 5 032 at `parse` on roslyn, runtime, aspnetcore, Avalonia, the mutations and the fuzz before the documentation comment parser) |
| declarations, members and types (4610df50) | 7051 | 0 of 17 075, 31 261, 10 188 and 4 085 corpus files; 4 of 122 741 mutations, 8 of 20 000 fuzz inputs |
| the last three causes, each reduced to `tests/oracle/` (43519b4f) | 7051 | 0 everywhere, at Latest and at Preview |
| the dumps in the gate, strict goldens (174ecb80) | 7051 | the gate's step 10; the gate took 20-23 minutes, up from 9 |

The lexer alone (`ksdump --lex --bench`, one thread, `--doc none`, a7434734) at load 30-57: medians 47-237 MB/s per
corpus, the best rounds 68-298 MB/s; at the quietest moment 236 MB/s on Avalonia and 426 MB/s on roslyn's largest
file (6.6 MB).

### C# parse throughput and robustness (43ed5b13), 2026-11-02

`ksdump --bench --lang csharp` (parse and flatten, files read before timing) over the four corpora of
tools/csharp-oracle, alternating in each round with tree-sitter-c-sharp 0.23.5 on tree-sitter 0.25.10 (C core, outside
the workspace as for Java: one `tree_sitter::Parser` per thread, the same bytes, `parse(text, None)`, fat LTO). Each
run is `--repeat 3` and gives its median; one thread reports the summed parse time, 16 threads the wall time. 3
rounds; the 5-minute load at the start of the runs was 5-39, so the figures are lower bounds and the rounds vary by up
to 1.5×. Medians over the rounds, MB/s, with the median of the per-round ratios:

| corpus (files, MB) | ours, 1 thread | tree-sitter, 1 thread | ratio | ours, 16 threads | tree-sitter, 16 threads | ratio |
|---|---|---|---|---|---|---|
| dotnet/roslyn (17 075, 249.94) | 113.4 | 16.5 | 5.77× | 1245.1 | 167.6 | 8.53× |
| dotnet/runtime (31 267, 426.88) | 59.2 | 11.7 | 5.52× | 443.0 | 58.6 | 5.44× |
| dotnet/aspnetcore (10 188, 62.04) | 65.3 | 12.5 | 5.29× | 1027.7 | 111.6 | 9.21× |
| Avalonia (4 085, 23.59) | 83.2 | 14.5 | 5.52× | 1016.7 | 115.3 | 8.80× |

The runtime corpus at 16 threads waits for its largest files: `hugeexpr1.cs` (24.12 MB, lines 600 parentheses deep)
parses in 1.1 s alone (21.3 MB/s). On 93d30b1a, before "Replay finished tuple type scans", it took 21.7 s and the
corpus ran at 6.7 MB/s on one thread (load 20); instructions per parse round (one thread, the difference of
`--repeat 2` and `--repeat 1` under `/usr/bin/time -l`) went 649 → 155 G on runtime, 56.5 → 55.9 G on roslyn,
18.2 → 17.2 G on aspnetcore and 6.85 → 6.39 G on Avalonia, all trees unchanged (the gate; the 6 runtime files without
an oracle dump byte-identical to 93d30b1a's dumps). That is 0.22-0.36 G instructions per source MB, against Kotlin's
0.205 and Java's 0.201 ("Current").

The trees (deterministic): roslyn 55 559 618 nodes, 5.00 MB of tree per source MB, 22.5 bytes a node; runtime
162 757 775, 7.86 MB, 20.6 (the largest tree 248.2 MB); aspnetcore 20 660 580, 6.99 MB, 21.0; Avalonia 8 001 383,
7.11 MB, 20.9. tree-sitter-c-sharp reports an error (`has_error`) in 927, 1152, 151 and 96 files; Roslyn in 41, 29,
16 and 2.

Robustness, `tests/robustness.rs` at load 15-40 on 43ed5b13 (see "C# nesting and work"): before that commit nested
local functions overflowed a 1 MB stack at 1000 levels and `a?.b?.…` at 100 000; 100 000 levels of `[[[…` took
10.8 s, of `[..` 12.0 s, of `a?[` 2.9 s (161 ms at 100), of stray `}` after a class 3.7 s, of excluded `#if` 3.4 s,
and 800 nested parentheses 22.7 ms (quadratic). Now the slowest of the 153 forms at 100 000 levels is `a?[` at
210 ms, of the 101 chains `join` clauses at 118 ms.

A first profile of the roslyn corpus (`scripts/profile.py`, one thread, 43ed5b13's parent): flattening the green tree
13.6 % self (`Flattener::run`, with `Walk` 3.5 %), the lexer about 25 % (`lex`, identifiers, trivia, whitespace),
`current_kind` 9.1 %, `push_node` 5.8 % plus its `memmove`. Bounding `PreLex`'s UTF-16 count of the window, 3.8 % of
`SyntaxParser::new` in that profile, moved no instructions beyond noise (−1.8 % to +0.6 % per corpus) and was not
kept.

The gate passed on 43ed5b13.

### Current, 72c8c11 (the code of 7ce233e), 2026-11-02

Against 9b8c26f, the master before the builder core batch, rebuilt in a temporary worktree; both binaries are plain
`cargo build --release --locked` builds with the workspace profile. Load was 1.2-4.8 (a quiet machine).
Instructions and cycles are the primary metric: per parse round, the difference of `ksdump --bench --repeat 4` and
`--repeat 1` under `/usr/bin/time -l`, divided by 3, binaries alternating, 2 or 3 passes each (at 16 threads, all
threads of the process). Wall is secondary: `scripts/ab.sh`, alternating, medians; at 16 threads, wall time per round.

| measure | Kotlin (55 501 files, 172.91 MB) | Java (68 263 files, 215.75 MB) |
|---|---|---|
| instructions, 1 thread | 54.19 → 35.39-35.40 G (−34.7 %) | 75.38-75.40 → 43.47 G (−42.3 %) |
| cycles, 1 thread | 9.96-9.98 → 6.81-6.84 G (−31.6 %) | 13.79 → 7.88-7.91 G (−42.7 %) |
| instructions, 16 threads | 53.87-54.21 → 35.41 G | 75.24-75.44 → 43.28-43.54 G |
| cycles, 16 threads | 12.18-12.19 → 8.26-8.28 G (−32.1 %) | 15.93-16.00 → 9.67-9.76 G (−39.1 %) |
| wall, 1 thread (5 rounds, load 1.5-1.7) | 75.4 → 111.3 MB/s (×1.48) | 69.1 → 120.0 MB/s (×1.74) |
| wall, 16 threads (3 rounds, load 1.5-4.8) | 956.5 → 1414.9 MB/s (×1.48) | 913.2 → 1488.4 MB/s (×1.63) |
| body-less, 1 thread (3 rounds, load 1.9-2.2) | 149.6 → 231.8 MB/s (×1.55) | 127.2 → 265.2 MB/s (×2.08) |
| lexer alone (`--lex --bench --repeat 7`, 2 passes) | 5.80 → 3.11 G, cycles 1.46-1.48 → 1.01-1.04 G; 539 → 770 MB/s | 3.73-3.86 G, cycles 1.16 G, 852 MB/s (9b8c26f cannot bench the Java lexer) |
| doc-comment lexer alone (`--docs`, one run) | 453.5 MB/s over 9.54 MB of KDoc | 494.3 MB/s over 9.37 MB of JavaDoc |

B was faster in every round. The first pass of 9b8c26f's Kotlin (53.79 G) and Java (73.41 G, 12.40 G cycles) is
below the others, an outlier of its `--repeat 1` run; the other passes gave 54.19 G and 75.38-75.40 G. At 16 threads a
round costs 21-23 % more cycles than on one thread (8.27 against 6.82 G Kotlin) for the same instructions: the cores
share caches and memory bandwidth.

The trees are the same on both commits (deterministic, single values):

| | Kotlin | Java |
|---|---|---|
| nodes | 63 742 904 | 85 270 521 |
| tree per source MB (text included) | 7.64 MB | 8.12 MB |
| bytes a node | 20.7 | 20.6 |
| nodes per source KB | 368.6 | 395.2 |
| largest tree | 3.4 MB | 9.3 MB |
| body-less: nodes, tree per source MB | 31 240 422, 4.26 MB | 38 558 344, 4.24 MB |

The preorder walk measures 2.16 → 2.18 ns/node and the linear scan 1.57 → 1.53 ns/node (Kotlin, one run each, the
same checksum `96e0e5f40acc13a0`); as a share of the faster parse the walk is now 9.0 % (6.1 % before).

`scripts/pgo.sh` (training on the goldens) on 72c8c11 against the plain build, alternating, 2 passes: Kotlin
35.01-35.38 → 31.06-31.07 G (−12.2 %), cycles 6.85-6.86 → 6.15-6.17 G; Java 43.16-43.47 → 37.79-37.80 G (−13.0 %),
cycles 7.92-7.94 → 6.92-6.94 G. `scripts/ab.sh`, 3 rounds at load 1.4-2.0: Kotlin 112.7 → 124.3 MB/s (×1.10), Java
121.0 → 137.2 MB/s (×1.13). Its `--walk` checksums (kind, range and depth of every node) and tree bytes equal the
plain build's on both corpora (`96e0e5f40acc13a0`, `c7e0ef63cac45006`); the gate itself did not run on it.

The gate passed on 72c8c11 and on the commit carrying this section.

### c796e73, 2026-11-01

There were 3 alternating rounds. In each round `ksdump --bench --repeat 3` (or `ts-bench … 3`) ran with 1 thread,
then tree-sitter-kotlin with 1 thread, then 16 threads, then `--walk`. The figures are medians over rounds of each
run's median. Load was 24 in round 1, 19 in round 2 and 10 in round 3. Round 1 ran at about half speed for both
parsers.

| measure | rounds | median |
|---|---|---|
| parse, 1 thread | 38.6 / 75.3 / 75.4 MB/s | **75.3 MB/s** |
| tree-sitter-kotlin 0.25.10 (C core), 1 thread, same files | 11.9 / 20.7 / 19.9 MB/s | **19.9 MB/s** (ours 3.8×) |
| parse, 16 threads | 863.5 / 965.2 / 951.3 MB/s | **951.3 MB/s** |
| preorder walk (kind, range, depth) | 2.19 / 2.15 / 2.14 ns/node | **2.15 ns/node**, 6.0 % of the parse |
| linear `descendants()` scan (kind, range) | 1.60 / 1.56 / 1.55 ns/node | **1.56 ns/node**, 4.3 % of the parse |
| body-less parse, 1 thread (load 4) | 142.3 / 141.1 / 140.0 MB/s | **141.1 MB/s**, 31 240 422 nodes |
| lexer alone (`--lex --bench`, load 4) | 480.0 / 477.4 / 476.8 MB/s | **477.4 MB/s** |

The tree is deterministic, so these are single values:

- 63 742 904 nodes;
- 7.64 MB of tree per source MB (text included);
- 20.7 bytes a node;
- 368.6 nodes per source KB;
- the largest tree is 3.4 MB;
- the body-less tree is 4.25 MB per source MB.

An earlier set of single runs at load 8-24 gave a 1-thread median of 44.5 MB/s for ours and 10.9 MB/s for
tree-sitter. Ours was 3.6-4.1× faster in every round.

The gate passed on c796e73 in 62 s.

### The workspace split (child of 26d5703), 2026-11-01

The crate split, `psi-builder` generic over `Language`, against 26d5703, both `ksdump --bench` on the community files:

- the 25 `--features counters` counters, the 63 742 904 nodes, the 7.64 MB of tree per source MB, the `--walk`
  checksum and the body-less 31 240 422 nodes are identical;
- the first split binary measured 1 thread 75.1 → 74.0 MB/s (B/A 0.985, 11 alternating rounds, A faster in 10, load
  3.3-5.1). Its code had changed in two places: `skip_whitespace`, inlined before, was a call on every cursor move,
  and `getJoinedTokenType`, a call before, was forced inline into every `tt()`. With `#[inline(always)]` on the first
  and no attribute on the second the code is within 1 KB of 26d5703's (697 064 vs 698 088 bytes of functions);
- that binary, alternating with 26d5703: 15 rounds at load 7.7-18 (median of per-round B/A 0.997), 21 rounds at load
  5.7-13.8 (69.4 vs 69.2 MB/s, median of per-round B/A 0.991). The machine was shared throughout; a quieter repeat is
  the open check;
- instructions retired of a whole `ksdump --bench` process (`/usr/bin/time -l`, file reading included), 5 runs each:
  26d5703 73.26-74.57 G, the split 73.48-74.69 G, equal within their run-to-run spread.

The gate passed on the split.

### The Java lexers and the parser skeleton (child of 000c6b3), 2026-11-01

- `ksdump --lex` against `tools/java-oracle/JavaLexOracle.java` (IntelliJ 271's java-syntax `JavaLexer`, and
  `JavaDocLexer` on every doc comment): 0 differing lines on the 68 263 community `.java` files (3 not UTF-8, both
  sides "unreadable"), the 19 116 golden mutations and 20 000 lexer fuzz inputs (`lex_fuzz.py`, seed `java-lex`);
  the 1036 golden inputs, untrimmed, equal too.
- `tests/lexer.rs`: 1036 of 1036 goldens have the leaf texts of their PSI dump (36 with tokens the parser joins, 55
  with relexed holders).
- Lexing the community `.java` files (215.75 MB) with both lexers and hashing every token: 5.2 s wall for
  `ksdump --lex`, 12.5 s for the Java oracle (one run each, file reads included, the oracle's JVM start too).
- Kotlin: `psi-builder`'s new parameters and methods change no Kotlin parse. Instructions retired of a whole
  `ksdump --bench` process, 3 runs each: before 73.59-74.67 G, after 74.64-74.70 G; `scripts/ab.sh`, 7 rounds at load
  9.5-17: per-round B/A 0.825-1.056, median 1.012 (too loaded to resolve 1 %).

The gate passed (including the Java lexer checks).

### The Java parsers (the merge of the declaration and statement lanes, child of 0f619c0), 2026-11-01

`DeclarationParser`, `FileParser`, `ModuleParser`, `ReferenceParser`, `PatternParser`, `JShellParser` (one lane),
`StatementParser`, `PrattExpressionParser`, `JavaDocParser` (the other), merged without conflicts; one method stays a
stub, `JavaParserUtil.stoppingBuilder(builder, condition)`, which no parser calls. Against IntelliJ 271's Java PSI:

- the goldens (`tests/goldens.rs`): 1036/1036 `.txt` and 1036/1036 `_node.txt`; `tests/doc_comments.rs`: 224/224
  doc comments of 219 goldens;
- the community `.java` files (`ksdump --node --out`): 68 260 of 68 263 dumped (3 not UTF-8, failures on both sides),
  0 differing files of either dump; 35.4 s wall, 557 MB peak RSS (one run, file writes included);
- the 19 116 golden mutations: 0 differing files of either dump;
- the 20 000 lexer fuzz inputs (`lex_fuzz.py`, seed `java-lex`, no cached PSI dumps; the oracle run once for this):
  parse without a panic, 19 989 equal, 11 differ in a dump label only (see "Open items").

The gate passed, with the community and mutation dumps of both kinds added to it.

### The Java dump labels of the lexer fuzz (child of 4631197), 2026-11-01

The 11 differing `java-lex` inputs were two labels. `PsiInlineDocTagImpl.toString()` concatenates a missing name
element as `null` (9 inputs, `/**{@ doc */`); `PsiPackageStatementImpl.getPackageName()` is
`JavaSourceUtil.getReferenceText`, which leaves out whitespace, comments and annotations and puts a space between two
leaves that would join into one identifier (2 inputs, `package pack1.@code pack2;`). The dump now ports both, and the
module name `PsiJavaModuleReferenceElementImpl.getReferenceText()` (the reference's children that are not whitespace or
comments; it walked into a doc comment's tokens before, no input had shown it). The oracle's dumps of the corpus
(both kinds, 5.8 s; 17 604 of 20 000 with a `PsiErrorElement`) are cached and gated: 0 differing files of either dump,
as for the 68 260 community files and the 19 116 golden mutations; goldens 1036/1036 `.txt` and `_node.txt`.

### Java parse throughput (child of 75baa59), 2026-11-02

`ksdump --bench` over the 68 263 community `.java` files (215.75 MB, `java-community-3caa0bf.list`, all parsed as files
at `JDK_28`; the 3 that are not UTF-8 parse as bytes), alternating in each round with tree-sitter-java 0.23.5 on
tree-sitter 0.25.10 (C core; `/tmp/ts-java-bench`, not committed: one `tree_sitter::Parser` per thread, the same files
with the same CRLF normalization read before timing, `parse(text, None)`, fat LTO). Each run is `--repeat 3` and gives
its median; one thread reports the summed parse time, 16 threads the wall time. 5 rounds of ours 1 thread,
tree-sitter 1 thread, ours 16 threads, tree-sitter 16 threads, ours body-less 1 thread. The machine was heavily shared:
load 14-37 (1-minute average at the start of each run), so these are lower bounds, not a quiet figure.

| measure | rounds | median |
|---|---|---|
| parse, 1 thread | 59.3 / 54.5 / 51.6 / 54.5 / 52.7 MB/s | **54.5 MB/s** |
| tree-sitter-java, 1 thread | 23.1 / 23.5 / 22.3 / 20.3 / 16.5 MB/s | **22.3 MB/s**; per-round ratio ours/tree-sitter 2.32-3.19, median **2.57×** |
| parse, 16 threads | 545.7 / 498.9 / 591.3 / 480.7 / 437.4 MB/s | **498.9 MB/s** |
| tree-sitter-java, 16 threads | 150.0 / 163.7 / 153.4 / 135.0 / 122.7 MB/s | **150.0 MB/s**; per-round ratio 3.05-3.85, median **3.56×** |
| body-less parse, 1 thread | 98.7 / 97.5 / 22.9 / 93.4 / 63.2 MB/s | **93.4 MB/s** (round 3 ran during a load spike) |

A single earlier pair at load 13.5: ours 62.2 MB/s 1 thread (`--repeat 3`), 680.0 MB/s 16 threads; tree-sitter-java
25.3 and 175.7 MB/s right after it at load 23. The deterministic figures:

- 85 270 521 nodes; 8.12 MB of tree per source MB; 20.6 bytes a node; 395.2 nodes per source KB; the largest tree
  9.3 MB;
- body-less: 38 558 344 nodes, 4.23 MB per source MB;
- preorder walk 2.12 ns/node (5.2 % of the parse), linear scan 1.63 ns/node (4.0 %), `--walk` checksum
  `c7e0ef63cac45006` (one run at load 13.5);
- tree-sitter-java reports an error (`has_error`) in 1898 files; PSI has a `PsiErrorElement` in 990.

Kotlin's 75.3 MB/s (c796e73) was measured at load 10-24 with one round at load 4 and is not comparable to these
figures; a Java profile and a quiet round are open items. The bench change itself leaves the Kotlin parse alone:
instructions retired per Kotlin parse round (half the difference of `--repeat 3` and `--repeat 1` processes, two runs
each) 53.4 and 54.1 G on 75baa59, 54.7 and 53.9 G with the generic bench; `scripts/ab.sh`, 9 rounds at load 37-62,
gave B/A 0.909 with per-round values from 9.0 to 58.6 MB/s, which resolves nothing.

The gate passed.

### The Java nesting guard (child of 61b4d1d), 2026-11-02

- Corpus, `scripts/ab.sh` against 61b4d1d, one thread, 11 alternating rounds at load 12-20 over the community `.java`
  files: 56.0 → 60.4 MB/s (median B/A of the rounds 1.08, B faster in all 11). The gain is `parseBlockLazy`'s
  count of the raw tokens in place of its loop of `tokenType` and `advanceLexer` over every code block. Kotlin, 9
  rounds at load 10-19: 69.1 → 69.1 MB/s (median of the per-round B/A 0.993; round 8's B ran at 39.6 during a spike).
- Trees: every dump of the gate's Java and Kotlin corpora unchanged; the deepest community `.java` file
  (`Parenth2.java`) reaches 1010 levels, 4 type argument lists and 4 annotations.
- Adversarial inputs on a 1 MB thread, `tests/robustness.rs`, at load 12-20: 80 forms nested 100 000 deep in 1.5 ms
  (`!a`) to 102 ms (`new A() { void f() { g(`, a chain of bodies); before the guard the parser's own recursions overflowed
  a 1 MB stack below 10 000 levels, and on a 1 GB stack 16 000 levels took up to 8.0 s (enum constant bodies) and
  `@A(f(` 16 levels 22 s.
  44 chains and lists of 100 000 in 2.0 ms (modifiers) to 65 ms (`a<b>c` × 100 000); `a < a < …` took 15 s for 16 000
  operators before (each `a` tried a type with every `<` after it). `/// [[[…]]]`: 100 000 brackets 8.9 ms, 55 s
  before.

The gate passed.

### The builder core batch (children of 9b8c26f), 2026-11-02

Ten commits on the builder, the cursor and the driver, none changing a parser decision: one array of balanced entries
for `bind` with the default binders' edges taken directly, one record per node while building; the semantic
layer's stack tops in fields and the token before the cursor walked as a slice; the cursor's token computed at every
move (`sig`, `checked` kept only for remaps); `bind`'s entry walk out of line over borrowed fields; no error message
through `processDone`; frames reset in place instead of builders moved; Java's block ends read from the file's brace
pairs; `bind` without a stack (parents from the tree, open markers stashed in it); a rolled-back tail cut off at
once; Kotlin's current token one read; Java's stopping builder compared by token. The gate passed on each commit;
the 25 counters are identical but the cursor-call counts RawTokenType and RawLookup.

Measured against 9b8c26f at load 1.6-2.2 (a quiet machine). Instructions and cycles of the parse itself, per round,
from `thread_selfcounts` around each parse (2 alternating pairs of 2 rounds each), and of a whole process as the
difference of `--repeat 3` and `--repeat 1` (`/usr/bin/time -l`, one run each); wall from `scripts/ab.sh`:

| measure | Kotlin | Java |
|---|---|---|
| instructions, in-process | 54.21 → 40.76 G (-24.8 %) | 75.40 → 59.13 G (-21.6 %) |
| cycles, in-process | 10.03-10.09 → 7.67-7.69 G (-23.6 %) | 13.72-13.77 → 10.60-10.67 G (-22.8 %) |
| instructions / cycles, process difference | 54.17 / 10.20 → 40.74 / 7.69 G | 75.39 / 13.82 → 58.63 / 10.62 G |
| 1 thread, 5 rounds | 76.6 → 100.1 MB/s (×1.31) | 69.4 → 89.9 MB/s (×1.30) |
| 16 threads, 3 rounds | 972 → 1278 MB/s (×1.31) | 916 → 1163 MB/s (×1.27) |

B was faster in every round. Per phase after the batch (calibrated `thread_selfcounts` around each phase, Kotlin /
Java, instructions in G): lex 5.8 / 17.7, file parse 8.8 / 8.1, chameleon parse 17.0 / 19.7, balance 3.8 / 5.7,
bind 5.5 / 7.9, copy out 0.5 / 0.7. Balancing and binding still take about 30 % of the cycles of both (instructions
per cycle about 3 there, 5 to 6 in the parsers): their branches on start or done entries and on the leaves between
them are data-dependent. Measured and dropped on the way: nodes written in place without a growth check (+0.55 G
Kotlin, +0.8 G Java), a per-builder table of whitespace runs for the default edges (+1.3 G Kotlin: it covers the
collapsed bodies' tokens too), more `inline(always)` on marker operations (+0.2 G, 165 KB of code), a specialized
end-of-production link (no change).

### Inlining as a profile does it (measured as a child of 9b8c26f), 2026-11-02

Instructions retired is the primary metric here: the machine ran at load 8-22 throughout, which moves wall time by
±15 % between runs but instructions by under 0.2 %. Each parse was wrapped in `thread_selfcounts` (instructions and
core cycles of the parsing thread alone, summed per round; a patch to the bench under `/tmp`, not committed: it needs
`unsafe`). Per round, one thread, master 9b8c26f → this change:

| | Kotlin (172.91 MB) | Java (215.75 MB) |
|---|---|---|
| instructions | 54.27 → 52.13 G (−3.9 %) | 75.47 → 68.92 G (−8.7 %) |
| core cycles, median of 5 alternating pairs | 10.60 → 9.97 G (−6 %) | 14.19 → 12.80 G (−10 %) |
| `scripts/ab.sh` wall | 62.5 → 62.2 MB/s (9 rounds, B/A 0.995, noise) | 56.9 → 63.2 MB/s (7 rounds, B/A 1.111) |

Where the instructions went, step by step (each against the step before; a step that gained nothing was dropped):
`char_at` split and the Java letter tests inlined, Java −4.1 G, Kotlin −0.5 G; the marker and lookahead methods,
Kotlin −0.75 G, Java −1.0 G, then `#[inline(always)]` on the ones still called, −0.7 / −0.8 G; the Java builder's
wrappers, Java −0.6 G; Kotlin's wrappers, Kotlin −0.3 G; `#[cold]` on errors, Java −0.2 G. Dropped: `#[inline]` on
the Kotlin lexer's helpers (0), `soft_keyword_by_text` (0), a `match` for `token_to_binary_precedence` (0), a `match`
for `LanguageLevel::is_preview` (−0.05 G), `#[inline(always)]` on `done` (+0.8 G on both), `current_types`' recompute
out of line (+0.3 G Kotlin). Every hint is also kept by a profile-guided build: PGO on top measures 44.73 G Kotlin and
57.05 G Java, against 45.07 and 57.37 G for PGO on master. Binary 1384 → 1666 KB. The gate passed.

It landed on the builder core and lexer batches, where it measured Kotlin 36.00 → 35.40 G (−1.7 %) and Java
44.23 → 43.48 G (−1.7 %), binary 1140 → 1338 KB (see "The radical batch integrated").

### The radical batch integrated (9b8c26f → a49adb7), 2026-11-02

Four branches, each measured against 9b8c26f alone and then landed one at a time, re-measured on what was already in:
the builder core batch (12 commits through ca327ce), the lexers (be86ab9 tooling, dd84487 file lexers, 194a628 doc
lexers), the inline and cold hints (7ce233e) and `scripts/pgo.sh` (a49adb7). The gate passed on every code commit's
own tree. Instructions and cycles per parse round are the difference of `ksdump --bench --repeat 4` and `--repeat 1`
under `/usr/bin/time -l`, divided by 3; the binaries alternate, two passes at load 1.4-2.6 (a quiet machine), so a
value repeats within about 0.4 G; MB/s is the bench's own last round in the same runs:

| step | Kotlin instructions | Kotlin cycles | Kotlin MB/s | Java instructions | Java cycles | Java MB/s |
|---|---|---|---|---|---|---|
| 9b8c26f | 54.19 G | 9.89-10.07 G | 75.8 | 75.09-75.40 G | 13.76-13.77 G | 69.7 |
| builder core batch (ca327ce) | 40.74 G (−24.8 %) | 7.66-7.76 G | 99.5 | 58.82-59.12 G (−21.6 %) | 10.63-10.72 G | 89.9 |
| file lexers (dd84487) | 38.04 G (−6.6 %) | 7.27-7.29 G | 105.0 | 45.29 G (−23.4 %) | 8.13-8.15 G | 117.6 |
| doc lexers (194a628) | 36.00 G (−5.4 %) | 7.01-7.05 G | 108.7 | 44.23 G (−2.3 %) | 8.00-8.04 G | 119.9 |
| inline and cold hints (7ce233e) | 35.40 G (−1.7 %) | 6.78-6.80 G | 112.4 | 43.48 G (−1.7 %) | 7.85-7.88 G | 121.6 |
| the same through `scripts/pgo.sh` (goldens) | 31.07 G (−12.2 %) | 6.16-6.17 G | 124.3 | 37.68-37.79 G (−13.2 %) | 6.88-6.96 G | 137.9 |

In all, 9b8c26f → 7ce233e: Kotlin −34.7 % instructions and −32.6 % cycles, Java −42.3 % and −43.0 %. Wall,
`scripts/ab.sh` 9b8c26f against 7ce233e at load 1.2-2.9, B faster in every round: one thread Kotlin 77.4 → 114.4
MB/s (5 rounds, ×1.48), Java 70.5 → 122.7 MB/s (×1.74); 16 threads Kotlin 972 → 1419 MB/s (3 rounds, ×1.46), Java
922 → 1492 MB/s (×1.62). Java now parses faster per byte than Kotlin.

Not landed, from the same round of branches:

- **One record per node in the finished tree** (kinds beside a `[u32; 4]` of start, end, parent and subtree end) and
  **opening and closing composites in one bind loop, open markers stashed in the tree**: the builder core batch has
  both ideas (one build record per node split in the copy-out, a stackless bind), and the branch's own measurement
  found the split and the two-column layout equal in instructions (53.22 against 53.29 G). The two-column tree walked
  2.09 against 2.15 ns per node (`--walk`, two runs each, load 1.5), 4 ms a round, 0.2 % of a parse; not worth the
  conflict with the batch's tree builder.
- **`Parser::parse_parallel`**, one file's chameleons parsed on scoped threads and stitched in preorder, the file
  lexed in parts from 64 KB: exact on every gate file and 1.5-1.6× lower latency on the 40 largest files of each list
  (4-8 threads), but no throughput, since consumers parse files in parallel already, and its driver code was written
  against the frames the builder core batch replaced. See "Rejected".

### History (1 thread / 16 threads, alternating A/B medians at each step)

| commit | change | 1 thread MB/s | 16 threads MB/s | other |
|---|---|---|---|---|
| 1c8bcfa | baseline | 39.0 | 455.5 | tree 15.86 MB per source MB, 43.0 B/node; 10.7M allocations per pass |
| 50ac0fe | byte-match keyword lookups | 39.2 → 46.2 | 474.7 → 552.2 | |
| efbfcbc | token text as bytes | 46.5 → 48.8 | 553.7 → 577.7 | |
| ed452bd | current-token cache, handle truncation cache | 48.1 → 52.0 | 561.8 → 582.7 | RawTokenType calls per token 12.21 → 1.68 |
| a7d433e | `#[inline(always)]` cursor chain | 51.4 → 61.2 | 583.9 → 674.3 | binary 734 → 1016 KB |
| f3aa172 | raw brace scan for lazy blocks | 61.7 → 63.7 | 683.0 → 698.0 | Advance per token 1.28 → 0.77 |
| 6871816 | per-thread `Parser` buffers | 62.4 → 67.8 | 682.5 → 833.7 | |
| bf052ef | bind from the production, 20-byte markers | 67.4 → 70.6 | 800.6 → 881.3 | |
| a99fb45 | tree copied out exactly sized | 71.7 → 72.4 | | 11.32 MB per source MB; 179 779 allocations per pass |
| 279585f … 7fc4e69 | in-place chameleons, preorder columns | 71.5 → 71.9 | | 7.64 MB per source MB; walk 3.62 → 2.23 ns/node |
| a405a03 | ASCII identifier table | 71.9 → 73.9 | 918.0 → 926.1 | lexer alone 441.3 → 507.7 MB/s |
| 36b3b17 | the nesting limit, frames dropping bound tokens | 74.4 → 75.0 (9 rounds, load 3.4-5.1) | | 0 trees changed; 100 000 levels of 22 forms parse on 1 MB |
| child of 36b3b17 | linked production list | 72.7 → 74.2 (11 rounds, load 4.7-10.3) | | `a+a+…` × 100 000: 4.67 s → 22 ms; 18 chains 14-45 ms |
| 9b8c26f → the builder core batch | balanced entries, eager cursor, frames in place, brace pairs, stackless bind (10 commits) | 76.6 → 100.1 (5 rounds, load 1.6-2.2); Java 69.4 → 89.9 | 972 → 1278; Java 916 → 1163 | instructions per round 54.21 → 40.76 G, Java 75.40 → 59.13 G |
| be86ab9 … 194a628 | byte-class lexing loops, keyword hashes, doc-comment runs (on the builder core batch) | 99.5 → 108.7; Java 89.9 → 119.9 (bench, alternating, load 1.4-2.6) | | instructions per round 40.74 → 36.00 G, Java 59.12 → 44.23 G |
| 7ce233e (measured first as a child of 9b8c26f) | inline and cold hints where a profile inlines | Kotlin 62.5 → 62.2 (9 rounds, load 9.5-13, noise); Java 56.9 → 63.2 (7 rounds, load 8-12) | | instructions per round Kotlin −3.9 %, Java −8.7 %; core cycles −6 %, −10 %; binary 1384 → 1666 KB; on the lexer batch −1.7 % both |
| 9b8c26f → 7ce233e | the radical batch integrated | 77.4 → 114.4 (5 rounds, load 1.2-1.7); Java 70.5 → 122.7 | 972 → 1419; Java 922 → 1492 | instructions per round 54.19 → 35.40 G, Java 75.40 → 43.48 G |
| 9b8c26f → 72c8c11 | the final measurement, the baseline rebuilt in a temporary worktree | 75.4 → 111.3 (5 rounds, load 1.5-1.7); Java 69.1 → 120.0 | 956.5 → 1414.9; Java 913.2 → 1488.4 | instructions per round 54.19 → 35.40 G, Java 75.40 → 43.47 G; lexer alone 539 → 770 MB/s; trees unchanged (see "Current") |

End to end: 1c8bcfa → f3aa172 measured 39.0 → 63.4 MB/s (×1.63), and f3aa172 → a99fb45 measured 60.8 → 69.1 MB/s
(×1.14, with 16 threads 684.2 → 917.0). 72c8c11's 111.3 MB/s on one thread and 1415 MB/s on 16 are about 2.9× and
3.1× the 1c8bcfa numbers. They are not an A/B against 1c8bcfa.

The Kotlin lexer alone measures 539 MB/s on 9b8c26f at load 2.4, against 477 MB/s for c796e73 at load 4 and
507.7 MB/s for a405a03, all over the same lexer source: the spread is machine state, not a measured change. On
72c8c11 it is 770 MB/s in the same alternating runs.

### Profile after batch 1 (f3aa172, `scripts/profile.py`, 1 thread)

These are self-time shares, before the batch 2 and 3 changes:

- build_tree 16.6 %, KotlinLexer::advance 7.6 %, index_of 6.8 %, mark 6.1 %, plain_identifier_end 5.4 %;
- memmove 4.8 %, insert_leaves 3.8 %, create_leaf 3.2 %, FlexKDocLexer::advance 2.7 %;
- malloc, free and realloc 3.5 % together.

The token cursor is no longer the dominant cost. Batches 2 and 3 targeted the bind, marker and allocation items.
There is no line-level profile of the current code: `atos` returned no line info in that session.

## What remains

### Open items

- **C# speed.** 0.215 G instructions per source MB on dotnet/runtime's files up to 4 MiB after "C# parse speed, the
  six commits after 25dac59f" (0.312 before), against Kotlin's 0.205 and Java's 0.201. On the Mac Studio at 20
  threads the stack measured 62.6 MB/s a thread on one quieter pass and 46.7 at load 12-17 (accio's target is 50):
  open are a second quiet 20-thread pass there, the side table and the token map's size measured there on their own,
  a new profile of the lexer and `LanguageParser` (now most of the time), and a quiet-machine run of the table in "C#
  parse throughput and robustness".
- **C# speculation below the budget.** `a?[0]?[0]…` (17 or more) and 100 000 `a < a < …` fail as too deep; Roslyn
  parses the former in seconds and would recurse 100 000 deep for the latter. A memo of `?[`'s speculation, as of
  tuple scans, would make the first polynomial.
- **Java speed.** One thread at 120.0 MB/s against Kotlin's 111.3 on 72c8c11 (load 1.5-1.7, "Current"): 0.201 G
  instructions per source MB against Kotlin's 0.205 G. The Java builder path still has no `--features counters`
  output.
- **Java's rolled-back lambda parameters.** `(a<b>c(…` looks like a typed lambda: its parameter list is parsed, with
  PSI's error recovery to the `)` that ends it, then rolled back, at every level. So each level inside the nesting
  limit scans the rest of the text: 10 000 levels of `(a<b>c(` with `)) + 1` or `)) - 1` after take 0.8 s, 100 000
  levels 15-25 s (linear in the text, times about 300 levels). Bounding it needs the attempt to know where its list
  ends without parsing it; the recovery parses references there (and remaps tokens that stay remapped after the
  rollback), so it is left as PSI's.

- **The split's speed on a quiet machine.** Its A/B ran at load 6-18; repeat `scripts/ab.sh` against 26d5703 at load
  under 4 before attributing a later change's small delta.

- **Copies into chameleon frames.** Each chameleon still copies its tokens from the file's (suspended frames then
  drop what they have bound). Nested chameleons over T tokens copy O(depth · T): 100 000 nested `{ fun g() ` (250
  frames at the limit) take 99 ms on 36b3b17, 2026-11-01. Sharing the file's tokens would remove it (see "Remaining
  copies").
- **Remaining copies.** `lex_types` is copied into each chameleon builder (remap mutates it), and the text is copied
  into the tree's `Arc` per file. Sharing the file's token starts measured 0.999× and was reverted; revisit it
  together with frame memory.
- **Next profile.** The current code has not been profiled at line level. Take one before choosing further cursor
  work.

### Rejected

| idea | why |
|---|---|
| Parsing bodies eagerly in the file builder | Not equivalent to PSI's fresh chameleon builder (newline and join stacks, binders and error dedup at the boundary, "Expecting '}'" dedup at the last lexeme) |
| A brace-match table from the lexer | The raw `lex_types` scan is already a byte-compare loop; at most about 1 %, plus memory per token |
| A soft-keyword id per token (rust-analyzer's contextual kind) | `soft_keyword_by_text` is about 1 % of samples after the byte match |
| A `next_significant` table or a newline-before bit | The token cache shrank their targets; revisit only if a profile shows more than 3 %. A chameleon's token 0 needs a special case |
| Production tombstones instead of `Vec::remove` in drop | Superseded by the linked production, whose `drop` unlinks in constant time |
| rust-analyzer's `forward_parent` / a marker-system rewrite | Breaks the faithful MarkerProduction port for about 5 %; `rollbackTo` a preceding marker has no creation-order equivalent. The linked production keeps the port and removes the quadratic case |
| Biome-style trivia on tokens | PSI puts whitespace and comments as leaves where binders decide; the dump would differ |
| Ruff's lazy lexer with checkpoints | PsiBuilder needs the whole token array for lookahead, rollback by index and chameleon token reuse |
| oxc's 256-entry byte-handler table and pointer `byte_search` | Need `unsafe`; the safe part (the ASCII table) is built |
| Exact highlighting from the lexer alone | Soft keywords and `?.`/`?:`/`!!` come from the parser; a lexer-only pass is approximate only |
| Lazy expansion through interior mutability (`expand(&self)`) | Conflicts with `Send + Sync` and the preorder arena; `parse_body` returns separate trees instead |
| Parsing one file's chameleons in parallel | Files take milliseconds and consumers already parallelize across files. Built once (`parse_parallel`, a branch on 9b8c26f: chameleons on scoped threads into their own buffers, stitched in preorder, gate-exact): 1.5-1.6× lower latency on the largest files, no throughput; not landed over the builder core batch's frames (see "The radical batch integrated") |
| mimalloc inside the crate | Needs a C toolchain; it is the consumer's choice |
| C#: quick-scanned tokens sharing one record, as Roslyn's map shares its green token | Slower on one thread in the prototype, and flatten would have to guess where a shared token's text is; the map holds the token's shape and a hit pushes new records instead |
| C#: both trivia lists of a token packed into one run of `trivia_refs` (36-byte token records) | +4 % instructions (the split on every access) |
| C#: exact subtree node counts in the green records, so flatten sizes the tree exactly | Estimated under 1 %: `tree_node_estimate` overshoots by the garbage and the second walk is rare (14 runtime files) |
| Shared token starts for chameleons | Measured 0.999× (1 thread) and 0.995× (16 threads); a copy into a warm buffer costs as much as the extra arithmetic on each access |
